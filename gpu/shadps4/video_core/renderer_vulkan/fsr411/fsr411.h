// SPDX-License-Identifier: GPL-2.0-or-later
// gow3: FSR 4.1.1 (AMD's INT8 model of the 4.1.1 upscaler DLL) on Vulkan, replaying what the DLL
// does on D3D12 (docs/upscaler.md; tools/fsr4cap records it, tools/fsr4cap/extract.py builds
// the asset sets: SPIR-V of every pass and the model weights).
//
// Plain Vulkan (no renderer types) so that the game (vk_fsr4.cpp) and the benchmark
// (tools/fsr4_bench.cpp) share it. Frames record into the caller's command buffer; the caller
// keeps the images in layout General and makes sure that the frame recorded kFramesInFlight
// frames earlier has completed before recording a new one (constant buffer ring).

#pragma once

#include <cstdint>
#include <memory>
#include <string>

#include <vulkan/vulkan.h>

namespace Fsr411 {

constexpr uint32_t kFramesInFlight = 8;

struct Image {
    VkImage image = VK_NULL_HANDLE;
    VkImageView view = VK_NULL_HANDLE; ///< depth: a view of the depth aspect
    uint32_t width = 0, height = 0;
};

struct Frame {
    VkCommandBuffer cmdbuf = VK_NULL_HANDLE;
    Image color, depth, motion, output; ///< color RGBA16F, depth D32, motion RG16F (render pixels)
    uint32_t render_width = 0, render_height = 0;
    bool ultra_performance = false; ///< the second model (quality ratio 3)
    float jitter[2] = {};           ///< render pixels, as for FSR 3/4
    float motion_scale[2] = {1.0f, 1.0f};
    float pre_exposure = 1.0f;
    float sharpness = 0.0f; ///< 0..1
    bool sharpen = false;
    bool reset = false;
    bool auto_exposure = true;
};

class Upscaler {
public:
    /// `dir`: the asset sets (t1080_m0 ... t2160_m1).
    Upscaler(VkPhysicalDevice physical, VkDevice device, std::string dir);
    ~Upscaler();

    /// Records one upscale; false with Error() set when it cannot (assets, device, sizes).
    bool Record(const Frame& frame);
    [[nodiscard]] const std::string& Error() const noexcept;
    /// The asset set and sizes in use, for logs ("t2160_m0 2260x1272 -> 3840x2160").
    [[nodiscard]] std::string Describe() const;

private:
    struct Impl;
    std::unique_ptr<Impl> impl;
};

} // namespace Fsr411
