// SPDX-License-Identifier: GPL-2.0-or-later
// gow3: FSR 4 (source-v07 INT8/DOT4 model, FSR-Vulkan provider) as an alternative to the
// FSR 3.1 context of TemporalUpscaler. It consumes the same inputs: HDR scene color, depth,
// motion vectors in render pixels (previous - current) and the jitter offset.
//
// The model is per preset (native, quality, balanced, performance, ultraperf) and per output
// tier; its SPIR-V passes and weights come from tools/fetch_fsr4_assets.sh (GOW3_FSR4_DIR overrides
// the fsr4_shaders directory). A preset change rebuilds the model graph.

#pragma once

#include <array>
#include <memory>
#include "common/types.h"
#include "video_core/renderer_vulkan/vk_common.h"

namespace Vulkan {

class Instance;
class Scheduler;

class Fsr4Upscaler {
public:
    Fsr4Upscaler(const Instance& instance, Scheduler& scheduler);
    ~Fsr4Upscaler();

    /// An image in layout General, written/read by compute shaders.
    struct Image {
        vk::Image image;
        vk::ImageView view;
        u32 width, height;
    };
    struct Frame {
        vk::CommandBuffer cmdbuf;
        Image color, depth, motion, output;
        u32 render_width, render_height;
        int preset; ///< Gow3Settings::Preset
        std::array<float, 2> jitter;
        float frame_ms, near_plane, far_plane, vertical_fov;
        float sharpness;
        bool sharpen, reset, auto_exposure;
    };

    /// Records FSR 4 into `frame.cmdbuf`; the output stays in General. False when FSR 4
    /// cannot run (Problem() says why; missing assets or device features are permanent).
    bool Record(const Frame& frame);

    /// Why the last Record failed, or null.
    [[nodiscard]] const char* Problem() const noexcept;
    /// Assets or device features are missing: FSR 4 will not run in this session.
    [[nodiscard]] bool Fatal() const noexcept;

private:
    struct Impl;
    std::unique_ptr<Impl> impl;
};

} // namespace Vulkan
