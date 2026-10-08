// SPDX-FileCopyrightText: Copyright 2024-2026 shadPS4 Emulator Project
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include <array>
#include <chrono>
#include <cstdlib>

#include "gow3_toggles.h"
#include "common/div_ceil.h"
#include "common/logging/log.h"
#include "core/emulator_settings.h"

#ifdef __unix__
#include "common/adaptive_mutex.h"
#else
#include "common/spin_lock.h"
#endif
#include "common/debug.h"
#include "common/types.h"
#include "video_core/buffer_cache/region_definitions.h"
#include "video_core/page_manager.h"

namespace VideoCore {

#ifdef PTHREAD_ADAPTIVE_MUTEX_INITIALIZER_NP
using LockType = Common::AdaptiveMutex;
#else
using LockType = Common::SpinLock;
#endif

/**
 * Allows tracking CPU and GPU modification of pages in a contigious 16MB virtual address region.
 * Information is stored in bitsets for spacial locality and fast update of single pages.
 */
class RegionManager {
public:
    explicit RegionManager(PageManager* tracker_, VAddr cpu_addr_)
        : tracker{tracker_}, cpu_addr{cpu_addr_} {
        cpu.Fill();
        gpu.Clear();
        writeable.Fill();
        readable.Fill();
    }
    explicit RegionManager() = default;

    void SetCpuAddress(VAddr new_cpu_addr) {
        cpu_addr = new_cpu_addr;
    }

    VAddr GetCpuAddr() const {
        return cpu_addr;
    }

    static constexpr size_t SanitizeAddress(size_t address) {
        return static_cast<size_t>(std::max<s64>(static_cast<s64>(address), 0LL));
    }

    template <Type type>
    RegionBits& GetRegionBits() noexcept {
        if constexpr (type == Type::CPU) {
            return cpu;
        } else if constexpr (type == Type::GPU) {
            return gpu;
        }
    }

    template <Type type>
    const RegionBits& GetRegionBits() const noexcept {
        if constexpr (type == Type::CPU) {
            return cpu;
        } else if constexpr (type == Type::GPU) {
            return gpu;
        }
    }

    /**
     * Change the state of a range of pages
     *
     * @param dirty_addr    Base address to mark or unmark as modified
     * @param size          Size in bytes to mark or unmark as modified
     */
    template <Type type, bool enable>
    void ChangeRegionState(u64 dirty_addr, u64 size) noexcept(type == Type::GPU) {
        RENDERER_TRACE;
        const size_t offset = dirty_addr - cpu_addr;
        const size_t start_page = SanitizeAddress(offset) / TRACKER_BYTES_PER_PAGE;
        const size_t end_page =
            Common::DivCeil(SanitizeAddress(offset + size), TRACKER_BYTES_PER_PAGE);
        if (start_page >= NUM_PAGES_PER_REGION || end_page <= start_page) {
            return;
        }

        RegionBits& bits = GetRegionBits<type>();
        if constexpr (type == Type::CPU && enable) {
            CountWriteFaults(start_page, end_page);
        }
        if constexpr (enable) {
            bits.SetRange(start_page, end_page);
        } else {
            bits.UnsetRange(start_page, end_page);
        }
        if constexpr (type == Type::CPU) {
            UpdateProtection<!enable, false>();
        } else if (EmulatorSettings.GetReadbacksMode() == GpuReadbacksMode::Precise) {
            UpdateProtection<enable, true>();
        }
    }

    /// gow3: after a guest write fault, also unprotects the other pages of the window that
    /// are protected for CPU writes and hold no GPU-modified data (see MarkFaultWindow).
    void ExtendWriteFault(VAddr window_addr, u64 size) {
        const size_t offset = window_addr - cpu_addr;
        const size_t start_page = SanitizeAddress(offset) / TRACKER_BYTES_PER_PAGE;
        const size_t end_page =
            Common::DivCeil(SanitizeAddress(offset + size), TRACKER_BYTES_PER_PAGE);
        if (start_page >= NUM_PAGES_PER_REGION || end_page <= start_page) {
            return;
        }
        RegionBits add(~(gpu | writeable), start_page, end_page);
        if (add.None()) {
            return;
        }
        cpu |= add;
        UpdateProtection<false, false>();
    }

    /**
     * Loop over each page in the given range, turn off those bits and notify the tracker if
     * needed. Call the given function on each turned off range.
     *
     * @param query_cpu_range Base CPU address to loop over
     * @param size            Size in bytes of the CPU range to loop over
     * @param func            Function to call for each turned off region
     */
    template <Type type, bool clear>
    void ForEachModifiedRange(VAddr query_cpu_range, s64 size, auto&& func) {
        RENDERER_TRACE;
        const size_t offset = query_cpu_range - cpu_addr;
        const size_t start_page = SanitizeAddress(offset) / TRACKER_BYTES_PER_PAGE;
        const size_t end_page =
            Common::DivCeil(SanitizeAddress(offset + size), TRACKER_BYTES_PER_PAGE);
        if (start_page >= NUM_PAGES_PER_REGION || end_page <= start_page) {
            return;
        }

        RegionBits& bits = GetRegionBits<type>();
        // gow3: nothing modified in the range means nothing to clear or re-protect (CPU bits
        // always match `writeable` after a change); this runs for every buffer binding, so
        // it is checked before building the range mask.
        if (type == Type::CPU && !bits.AnyInRange(start_page, end_page) &&
            !Gow3Toggle::Disabled(Gow3Toggle::PageTrackingEarlyExit)) {
            return;
        }
        RegionBits mask(bits, start_page, end_page);

        if constexpr (clear) {
            bits.UnsetRange(start_page, end_page);
            if constexpr (type == Type::CPU) {
                KeepHotPagesModified(start_page, end_page);
                UpdateProtection<true, false>();
            } else if (EmulatorSettings.GetReadbacksMode() != GpuReadbacksMode::Disabled) {
                UpdateProtection<false, true>();
            }
        }

        for (const auto& [start, end] : mask) {
            func(cpu_addr + start * TRACKER_BYTES_PER_PAGE, (end - start) * TRACKER_BYTES_PER_PAGE);
        }
    }

    /**
     * Returns true when a region has been modified
     *
     * @param offset Offset in bytes from the start of the buffer
     * @param size   Size in bytes of the region to query for modifications
     */
    template <Type type>
    [[nodiscard]] bool IsRegionModified(u64 offset, u64 size) noexcept {
        RENDERER_TRACE;
        const size_t start_page = SanitizeAddress(offset) / TRACKER_BYTES_PER_PAGE;
        const size_t end_page =
            Common::DivCeil(SanitizeAddress(offset + size), TRACKER_BYTES_PER_PAGE);
        if (start_page >= NUM_PAGES_PER_REGION || end_page <= start_page) {
            return false;
        }

        const RegionBits& bits = GetRegionBits<type>();
        return bits.AnyInRange(start_page, end_page);
    }

    LockType lock;

private:
    /**
     * Notify tracker about changes in the CPU tracking state of a word in the buffer
     *
     * @param word_index   Index to the word to notify to the tracker
     * @param current_bits Current state of the word
     * @param new_bits     New state of the word
     *
     * @tparam track True when the tracker should start tracking the new pages
     */
    template <bool track, bool is_read>
    void UpdateProtection() {
        RENDERER_TRACE;
        RegionBits mask = is_read ? (~gpu ^ readable) : (cpu ^ writeable);
        if (mask.None()) {
            return;
        }
        if constexpr (is_read) {
            readable = ~gpu;
        } else {
            writeable = cpu;
        }
        tracker->UpdatePageWatchersForRegion<track, is_read>(cpu_addr, mask);
    }

    // gow3: pages the guest writes again and again (per-frame constants, skinning output)
    // cost a protection fault in the writing thread plus an mprotect with TLB shootdowns on
    // every upload. After HotFaults faults a page stays writable and counts as always CPU
    // modified, so it is uploaded on every use instead. The set is rebuilt every HotPeriod.
    // Opt-in (GOW3_HOT_PAGES=1): in Hunter's Nightmare the set grew to ~14k pages (56 MB), each
    // re-uploaded on every binding, which cost far more than the faults (33 FPS) and preceded
    // a GPU ring timeout.
    static bool HotPagesEnabled() {
        static const bool enabled = [] {
            const char* env = std::getenv("GOW3_HOT_PAGES");
            return env && env[0] == '1';
        }();
        return enabled && !Gow3Toggle::Disabled(Gow3Toggle::HotPages);
    }
    // A short period re-protects thousands of pages at once and each faults again before it is
    // hot, a fault storm (a 2 s period gave ~20k faults/s and frame spikes).
    static constexpr u8 HotFaults = 2;
    static constexpr auto HotPeriod = std::chrono::seconds(20);

    void CountWriteFaults(size_t start_page, size_t end_page) {
        const bool hot_enabled = HotPagesEnabled();
        for (size_t page = start_page; page < end_page; ++page) {
            if (writeable.Get(page)) {
                continue; // not protected: no fault
            }
            Gow3Stats::tracker_faults.fetch_add(1, std::memory_order_relaxed);
            if (!hot_enabled) {
                continue;
            }
            if (write_faults[page] < HotFaults) {
                ++write_faults[page];
                continue;
            }
            if (!hot.Get(page)) {
                if (num_hot == 0) {
                    hot_since = std::chrono::steady_clock::now();
                }
                hot.Set(page);
                ++num_hot;
                Gow3Stats::hot_pages.fetch_add(1, std::memory_order_relaxed);
            }
        }
    }

    void KeepHotPagesModified(size_t start_page, size_t end_page) {
        if (num_hot == 0) {
            return;
        }
        if (!HotPagesEnabled() ||
            std::chrono::steady_clock::now() - hot_since > HotPeriod) {
            // Re-protect them (on this upload) and start counting again.
            Gow3Stats::hot_pages.fetch_sub(num_hot, std::memory_order_relaxed);
            hot.Clear();
            write_faults.fill(0);
            num_hot = 0;
            return;
        }
        cpu |= RegionBits(hot, start_page, end_page);
    }

    std::array<u8, NUM_PAGES_PER_REGION> write_faults{};
    RegionBits hot{};
    u32 num_hot = 0;
    std::chrono::steady_clock::time_point hot_since{};

    PageManager* tracker;
    VAddr cpu_addr = 0;
    RegionBits cpu;
    RegionBits gpu;
    RegionBits writeable;
    RegionBits readable;
};

} // namespace VideoCore
