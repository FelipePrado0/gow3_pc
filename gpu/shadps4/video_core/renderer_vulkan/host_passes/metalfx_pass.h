// SPDX-License-Identifier: GPL-2.0-or-later
// gow3 (macOS): MetalFX's spatial scaler in FSR 1's place (gow3.ini screen_upscaler=metalfx,
// gow3_graphics.h). The display image is copied into an image Metal shares (vk_metalfx.h), the
// frame's commands so far are submitted, MetalFX enlarges the copy to the window on its own queue,
// and the frame's last submission, which reads the result (RCAS, post processing), waits for it.
// MetalFX is set up on first use. Where it cannot run, or fails, Render returns null and the
// presenter uses FSR 1; the log says why once.

#pragma once

#include <deque>
#include <memory>
#include <string>

#include "common/types.h"
#include "video_core/renderer_vulkan/vk_common.h"

namespace Vulkan {
class Instance;
class MetalFxUpscaler;
class Scheduler;
struct SubmitInfo;
} // namespace Vulkan

namespace Vulkan::HostPasses {

class MetalFxPass {
public:
    MetalFxPass();
    ~MetalFxPass();

    void Create(const Instance& instance);

    /// Enlarges `image` (`size` pixels, in ShaderReadOnlyOptimal, viewed as `format`) to
    /// `out_size`: records its copy, submits the scheduler's commands, queues the upscale and
    /// returns the result's view (ShaderReadOnlyOptimal, alpha 1) for the scheduler's new
    /// command buffer. Null when MetalFX cannot run it: then nothing was recorded.
    vk::ImageView Render(Scheduler& scheduler, vk::Image image, vk::Format format,
                         vk::Extent2D size, vk::Extent2D out_size);
    /// The frame's last submission waits for the upscale Render queued.
    void AddWait(SubmitInfo& info);

    /// Why MetalFX did not enlarge the last image it was asked to (null when it did).
    [[nodiscard]] const char* Problem() const {
        return problem;
    }

private:
    struct Config {
        u32 w, h, ow, oh;
        vk::Format format;
        bool operator==(const Config&) const = default;
    };
    /// Logs a new reason once; FSR 1 runs instead.
    vk::ImageView Unavailable(const std::string& why);

    const Instance* instance{};
    std::unique_ptr<MetalFxUpscaler> metalfx;
    bool created{}; ///< Create was tried
    bool broken{};  ///< unavailable or failed: FSR 1 for the rest of the run
    Config failed{}; ///< the sizes and format the scaler could not be set up for
    std::deque<std::string> reasons; ///< every reason given (stable for the menu's pointer)
    const char* problem{};
    u64 wait{}; ///< the upscale the frame's last submission waits for
};

} // namespace Vulkan::HostPasses
