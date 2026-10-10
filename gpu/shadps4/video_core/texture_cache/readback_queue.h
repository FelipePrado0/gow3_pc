#pragma once

#include <cstdint>
#include <cassert>
#include <deque>
#include <utility>

namespace VideoCore {

// Copies retire in submission order, including overlapping guest ranges.
template <typename Payload>
class ReadbackQueue {
public:
    struct Entry {
        std::uint64_t address, size, tick;
        Payload payload;
        bool valid = true;
        bool watched = true; ///< still protects its pages (see ReleaseOutside)
    };
    bool CanFit(std::uint64_t size) const {
        return entries.size() < 64 && size <= MaxBytes && bytes <= MaxBytes - size;
    }
    void Push(std::uint64_t address, std::uint64_t size, std::uint64_t tick, Payload payload) {
        assert(CanFit(size));
        entries.push_back({address, size, tick, std::move(payload)});
        bytes += size;
    }
    std::uint64_t RequiredTick(std::uint64_t address, std::uint64_t size) const {
        std::uint64_t tick = 0;
        for (const auto& entry : entries) {
            if (entry.valid && entry.watched && OverlapsPage(entry, address, size)) tick = entry.tick;
        }
        return tick;
    }
    template <typename Callback>
    void Cancel(std::uint64_t address, std::uint64_t size, Callback&& cancel) {
        for (auto& entry : entries) {
            if (entry.valid && OverlapsPage(entry, address, size)) {
                cancel(entry);
                entry.valid = false;
            }
        }
    }
    /// gow3: a CPU access to [address, address + size) that shares pages with small copies but
    /// not their bytes stops waiting for them: each such copy is unwatched (`release` drops its
    /// page protection) and still published when the GPU is done. Returns the copies released.
    template <typename Release>
    unsigned ReleaseOutside(std::uint64_t address, std::uint64_t size, std::uint64_t max_size,
                            Release&& release) {
        unsigned released = 0;
        for (auto& entry : entries) {
            if (entry.valid && entry.watched && entry.size <= max_size &&
                OverlapsPage(entry, address, size) &&
                !(address < entry.address + entry.size && entry.address < address + size)) {
                release(entry);
                entry.watched = false;
                ++released;
            }
        }
        return released;
    }
    template <typename Ready, typename Complete>
    void Retire(Ready&& ready, Complete&& complete) {
        while (!entries.empty() && ready(entries.front().tick)) {
            complete(entries.front());
            bytes -= entries.front().size;
            entries.pop_front();
        }
    }
    std::uint64_t Bytes() const { return bytes; }

private:
    static bool OverlapsPage(const Entry& entry, std::uint64_t address, std::uint64_t size) {
        if (!size) return false;
        const auto begin = entry.address & ~std::uint64_t{4095};
        const auto end = (entry.address + entry.size + 4095) & ~std::uint64_t{4095};
        return address < end && begin < address + size;
    }
    static constexpr std::uint64_t MaxBytes = 64ull << 20;
    std::deque<Entry> entries;
    std::uint64_t bytes = 0;
};
} // namespace VideoCore
