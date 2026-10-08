// SPDX-License-Identifier: GPL-2.0-or-later
// gow3: GOW3_GPU_PROFILE=1 — GPU time per render pass, dispatch and upscaler run. A timestamp is
// written where each of them starts (and at the frame end); the time to the next timestamp is
// charged to its label, barriers and copies recorded in between included. Results are read four
// frames later and printed every 5 s: GPU ms per frame by label.

#pragma once

#include <array>
#include <chrono>
#include <cstdio>
#include <string>
#include <unordered_map>
#include <vector>

#include "common/types.h"
#include "video_core/renderer_vulkan/vk_common.h"

namespace Vulkan {

class Instance;
class Scheduler;

class GpuProfiler {
public:
    /// The profiler when GOW3_GPU_PROFILE=1, else null.
    static GpuProfiler* Get() noexcept {
        return instance_ptr;
    }
    static void Init(const Instance& instance, Scheduler& scheduler);

    /// Starts a segment labelled `key`; `describe` names it the first time the key is seen.
    template <typename Describe>
    void Mark(u64 key, Describe&& describe) {
        if (!described.contains(key)) {
            described.emplace(key, describe());
        }
        WriteTimestamp(key);
    }

    /// The label of the open segment, for Resume after a nested segment (transfers).
    [[nodiscard]] u64 Current() const noexcept {
        return current;
    }
    /// Continues the segment of `key` (a label seen before) where a nested one ends.
    void Resume(u64 key) {
        WriteTimestamp(key);
    }

    /// Whether `other` is the scheduler it records into (the presenter has its own).
    [[nodiscard]] bool Records(const Scheduler* other) const noexcept {
        return other == &scheduler;
    }

    /// At the display pass: closes the frame's last segment, reads an old frame's results.
    void BeginFrame();

private:
    GpuProfiler(const Instance& instance, Scheduler& scheduler);
    void WriteTimestamp(u64 key);
    void Collect(u32 slice);
    void Print();

    static constexpr u32 NumSlices = 4;
    static constexpr u32 SliceQueries = 4096;
    static inline GpuProfiler* instance_ptr = nullptr;

    vk::Device device;
    Scheduler& scheduler;
    vk::UniqueQueryPool pool;
    double period_ns = 1.0;
    u32 slice = 0;
    std::array<std::vector<u64>, NumSlices> keys; ///< label of each timestamp but the last
    std::array<u32, NumSlices> used{};
    std::array<bool, NumSlices> pending{};
    std::unordered_map<u64, std::string> described;
    struct Total {
        double ms = 0;
        u64 segments = 0;
    };
    std::unordered_map<u64, Total> totals;
    u64 frames = 0;
    u64 current = 0;
    std::chrono::steady_clock::time_point window = std::chrono::steady_clock::now();
};

} // namespace Vulkan
