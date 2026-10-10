// SPDX-FileCopyrightText: Copyright 2026 shadPS4 Emulator Project
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include <array>
#include <atomic>
#include <cstdint>
#include <cstdio>

namespace VideoCore {

// gow3: who asked for a pending image readback to reach RAM. Only used by the
// GOW3_PERF_DIAG report, to find the consumer behind the remaining readback waits.
enum class ReadbackSource : std::uint8_t {
    Unknown,
    BufferObtain,
    BufferForImage,
    BufferUpload,
    CpuWrite,
    CpuRead,
    ImageDownload,
    GpuPartialWrite,
    ImageRefresh,
    CacheCleanup,
    ImageFree,
    Count,
};

constexpr const char* ReadbackSourceName(ReadbackSource source) {
    constexpr const char* names[] = {"unknown",      "buffer-obtain",  "buffer-for-image",
                                     "buffer-upload", "cpu-write",     "cpu-read",
                                     "image-download", "gpu-partial-write", "image-refresh",
                                     "cache-cleanup", "image-free"};
    return names[static_cast<std::size_t>(source)];
}

class ReadbackSourceStats {
public:
    struct Totals {
        std::uint64_t hits; // calls that overlapped a pending readback
        std::uint64_t waits; // hits that had to wait for the GPU
        std::uint64_t wait_ns;
        std::uint64_t max_wait_ns;
        std::uint64_t bytes;
    };

    void Record(ReadbackSource source, bool waited, std::uint64_t wait_ns, std::uint64_t bytes) {
        auto& entry = entries[Index(source)];
        entry.hits.fetch_add(1, std::memory_order_relaxed);
        entry.bytes.fetch_add(bytes, std::memory_order_relaxed);
        if (!waited) return;
        entry.waits.fetch_add(1, std::memory_order_relaxed);
        entry.wait_ns.fetch_add(wait_ns, std::memory_order_relaxed);
        std::uint64_t max = entry.max_wait_ns.load(std::memory_order_relaxed);
        while (wait_ns > max &&
               !entry.max_wait_ns.compare_exchange_weak(max, wait_ns, std::memory_order_relaxed)) {
        }
    }

    Totals Get(ReadbackSource source) const {
        const auto& entry = entries[Index(source)];
        return {entry.hits.load(std::memory_order_relaxed),
                entry.waits.load(std::memory_order_relaxed),
                entry.wait_ns.load(std::memory_order_relaxed),
                entry.max_wait_ns.load(std::memory_order_relaxed),
                entry.bytes.load(std::memory_order_relaxed)};
    }

    void Reset() {
        for (auto& entry : entries) {
            entry.hits.store(0, std::memory_order_relaxed);
            entry.waits.store(0, std::memory_order_relaxed);
            entry.wait_ns.store(0, std::memory_order_relaxed);
            entry.max_wait_ns.store(0, std::memory_order_relaxed);
            entry.bytes.store(0, std::memory_order_relaxed);
        }
    }

    // One line per source with activity, then resets the window.
    void Report(std::FILE* out) {
        for (std::size_t i = 0; i < entries.size(); ++i) {
            const auto source = static_cast<ReadbackSource>(i);
            const Totals t = Get(source);
            if (!t.hits) continue;
            std::fprintf(out,
                         "Readback source %s: hits %llu, waits %llu, wait %.3f ms, max %.3f ms, "
                         "%.2f MiB\n",
                         ReadbackSourceName(source), static_cast<unsigned long long>(t.hits),
                         static_cast<unsigned long long>(t.waits), t.wait_ns / 1e6,
                         t.max_wait_ns / 1e6, t.bytes / 1048576.0);
        }
        Reset();
    }

private:
    struct Entry {
        std::atomic<std::uint64_t> hits{0};
        std::atomic<std::uint64_t> waits{0};
        std::atomic<std::uint64_t> wait_ns{0};
        std::atomic<std::uint64_t> max_wait_ns{0};
        std::atomic<std::uint64_t> bytes{0};
    };

    static std::size_t Index(ReadbackSource source) {
        const auto index = static_cast<std::size_t>(source);
        return index < static_cast<std::size_t>(ReadbackSource::Count) ? index : 0;
    }

    std::array<Entry, static_cast<std::size_t>(ReadbackSource::Count)> entries{};
};

} // namespace VideoCore
