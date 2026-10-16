// SPDX-License-Identifier: GPL-2.0-or-later
// gow3: Apple MetalFX upscaling (macOS, vk_metalfx.mm): the temporal scaler for the experimental
// upscaler (GOW3_UPSCALER=metalfx), the spatial one in the presenter's FSR 1 place
// (screen_upscaler=metalfx, host_passes/metalfx_pass.h).
//
// MetalFX works on Metal textures. Under KosmicKrisp (Vulkan on Metal) every VkDeviceMemory is
// an MTLHeap, which VK_EXT_external_memory_metal exports: the images below get memory of their
// own, and an MTLTexture with the descriptor KosmicKrisp gives the VkImage is placed at offset 0
// of that heap, so both name the same memory with the same layout. MetalFX's output must be a
// private texture: it is blitted into the shared output image in the same Metal command buffer.
//
// KosmicKrisp exports no MTLSharedEvent (no VK_EXT_metal_objects), so the host relays the
// synchronization. Submit() encodes and commits the upscale at once, behind a wait for a Metal
// shared event of its own; a worker thread waits for the Vulkan submission that wrote the
// inputs (the scheduler's timeline semaphore) and then signals that event; the Metal command
// buffer's completion handler signals a Vulkan timeline semaphore that the submission reading
// the output waits for. No pixels pass through the CPU. Off macOS, Create() returns null.

#pragma once

#include <memory>
#include <string>

#include "common/types.h"
#include "video_core/renderer_vulkan/vk_common.h"

namespace Vulkan {

class Instance;

class MetalFxUpscaler {
public:
    /// The scaler an instance configures: Configure() (temporal) or ConfigureSpatial().
    enum Kind { Temporal, Spatial };
    /// Null when MetalFX's scaler of that kind or the Vulkan/Metal interop is unavailable;
    /// `problem` says why.
    static std::unique_ptr<MetalFxUpscaler> Create(const Instance& instance, std::string& problem,
                                                   Kind kind = Temporal);
    virtual ~MetalFxUpscaler() = default;

    /// Shared images, in the General layout while Metal uses them.
    struct Image {
        vk::Image image;
        vk::ImageView view;
    };
    /// Images and scaler for a `width` x `height` input and an `out_width` x `out_height` output.
    /// `color` is the scene color's format (RGBA16F for the HDR scene, RGBA8 for the tonemapped
    /// frame), `depth` the scene depth's (copied by depth aspect, so the formats match). Waits
    /// for queued upscales; the caller makes sure Vulkan is done with the previous images.
    virtual bool Configure(u32 width, u32 height, u32 out_width, u32 out_height, vk::Format color,
                           vk::Format depth) = 0;
    [[nodiscard]] virtual bool Matches(u32 width, u32 height, u32 out_width, u32 out_height,
                                       vk::Format color, vk::Format depth) const = 0;
    /// Spatial instead: the finished `width` x `height` image in `format` (the display's view
    /// format: RGBA8 or BGRA8 sRGB, or A2R10G10B10) enlarged to `out_width` x `out_height` in the
    /// same format. Color() is the input, Output() the result (its view reads alpha as 1); Submit
    /// ignores the Frame.
    virtual bool ConfigureSpatial(u32 width, u32 height, u32 out_width, u32 out_height,
                                  vk::Format format) = 0;
    [[nodiscard]] virtual bool MatchesSpatial(u32 width, u32 height, u32 out_width,
                                              u32 out_height, vk::Format format) const = 0;
    /// Inputs, written by Vulkan before Submit: color, depth (D32 or D32S8) and motion vectors
    /// (RG16F, render pixels, current to previous). Output: RGBA16F (temporal), read after
    /// the wait.
    [[nodiscard]] virtual Image Color() const = 0;
    [[nodiscard]] virtual Image Depth() const = 0;
    [[nodiscard]] virtual Image Motion() const = 0;
    [[nodiscard]] virtual Image Output() const = 0;

    struct Frame {
        float jitter_x, jitter_y; ///< render pixels, x right, y down (the port's jitter)
        bool reset;
    };
    /// Queues the upscale of the inputs written by the Vulkan work up to `tick` of `timeline`,
    /// which must be submitted. Returns the value of Semaphore() that work reading Output()
    /// must wait for. Called from the thread that records the frame (it encodes the upscale).
    virtual u64 Submit(vk::Semaphore timeline, u64 tick, const Frame& frame) = 0;
    [[nodiscard]] virtual vk::Semaphore Semaphore() const = 0;
    /// Waits until every queued upscale has completed.
    virtual void WaitIdle() = 0;
    /// A Metal command buffer failed (the output is then stale): the caller falls back.
    [[nodiscard]] virtual bool Failed() const = 0;
    /// Why the last Configure failed.
    [[nodiscard]] virtual const char* Problem() const = 0;
};

#ifndef __APPLE__
inline std::unique_ptr<MetalFxUpscaler> MetalFxUpscaler::Create(const Instance&,
                                                                std::string& problem, Kind) {
    problem = "MetalFX needs macOS";
    return nullptr;
}
#endif

} // namespace Vulkan
