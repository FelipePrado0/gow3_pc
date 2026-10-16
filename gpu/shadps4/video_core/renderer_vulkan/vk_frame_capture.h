// SPDX-License-Identifier: GPL-2.0-or-later
// gow3: frame analyzer. Creating the file named by GOW3_CAPTURE_TRIGGER records the next full
// frame: every render pass (targets, depth, draw count, shaders, sampled textures) and compute
// dispatch, in order, plus the buffer that was presented. Input for placing a temporal
// upscaler (scene color, depth, motion vectors, UI) in the game's frame. GOW3_CAPTURE_SPAN
// records across several display passes; GOW3_CAPTURE_DUMP (vk_rasterizer.cpp) writes chosen
// images as they are before a given pass.

#pragma once

#include <atomic>
#include <cstdio>
#include <string>
#include <vector>

#include "common/types.h"

namespace VideoCore {
struct ImageInfo;
}

namespace Vulkan {

class FrameCapture {
public:
    /// VideoOut thread, per flip: frame boundary, presented buffer, trigger file check.
    static void OnFlip(VAddr presented_address);

    /// VideoOut: a display buffer the game registered. A pass drawing into one starts a frame
    /// in the GPU command stream (the flip itself runs a frame behind).
    static void AddDisplayBuffer(VAddr address);
    static bool IsDisplayBuffer(VAddr address);

    /// GPU thread: whether passes are being looked at (armed or recording; cheap).
    static bool Active() {
        return state.load(std::memory_order_relaxed) != Idle;
    }
    /// GPU thread: whether a frame is being recorded right now.
    static bool IsRecording() {
        return state.load(std::memory_order_relaxed) == Recording;
    }
    /// GPU thread, at every draw/dispatch: starts or ends a recording at frame boundaries.
    static void Poll();

    /// `state`: the pass's census line (vk_pass_census.h), kept with the first pipeline that
    /// opened the pass.
    static void BeginPass(const VideoCore::ImageInfo* const* colors, u32 num_colors,
                          const VideoCore::ImageInfo* depth, std::string census);
    /// The index of the pass the last BeginPass started in the recording, or -1 (no new pass).
    static int NewPassIndex() {
        return new_pass_index;
    }
    static void Draw(u64 vs_hash, u64 ps_hash, u32 num_indices, u32 num_instances);
    static void Dispatch(u64 cs_hash, u32 x, u32 y, u32 z);
    static void Sampled(const VideoCore::ImageInfo& info, bool storage);
    /// Contents of a bound buffer (first 1 KiB), kept for small passes and a pass's first draw.
    static void Buffer(u64 stage_hash, u32 slot, VAddr address, const void* data, u64 size);
    static void Note(const char* text);

    /// In-game menu: records the next full frame (as the trigger file does).
    static void Request();
    /// What the last request did, for the menu: empty, "Capturing...", the file or an error.
    static std::string LastResult();

private:
    enum : u32 { Idle, Armed, Recording };
    static inline std::atomic<u32> state{Idle};
    static inline std::atomic<u64> flips{0};
    static inline std::atomic<VAddr> last_presented{0};
    static inline std::atomic<bool> requested{false};
    static inline int new_pass_index = -1; ///< GPU thread only
};

} // namespace Vulkan
