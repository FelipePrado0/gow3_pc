// SPDX-License-Identifier: GPL-2.0-or-later

#include <array>
#include <cstdio>
#include <utility>

#include "video_core/renderer_vulkan/host_passes/metalfx_pass.h"
#include "video_core/renderer_vulkan/vk_metalfx.h"
#include "video_core/renderer_vulkan/vk_scheduler.h"

namespace Vulkan::HostPasses {

MetalFxPass::MetalFxPass() = default;
MetalFxPass::~MetalFxPass() = default;

void MetalFxPass::Create(const Instance& instance_) {
    instance = &instance_;
}

vk::ImageView MetalFxPass::Unavailable(const std::string& why) {
    for (const std::string& reason : reasons) {
        if (reason == why) {
            problem = reason.c_str();
            return {};
        }
    }
    std::printf("Screen upscaler: MetalFX unavailable (%s); using FSR 1\n", why.c_str());
    problem = reasons.emplace_back(why).c_str();
    return {};
}

vk::ImageView MetalFxPass::Render(Scheduler& scheduler, vk::Image image, vk::Format format,
                                  vk::Extent2D size, vk::Extent2D out_size) {
    if (broken) {
        return {};
    }
    if (!created) {
        created = true;
        std::string why;
        metalfx = MetalFxUpscaler::Create(*instance, why, MetalFxUpscaler::Spatial);
        if (!metalfx) {
            broken = true;
            return Unavailable(why);
        }
    }
    if (metalfx->Failed()) {
        broken = true;
        return Unavailable("a Metal command buffer did not complete");
    }
    const Config config{size.width, size.height, out_size.width, out_size.height, format};
    if (!metalfx->MatchesSpatial(config.w, config.h, config.ow, config.oh, format)) {
        if (config == failed) {
            return {}; // set up for these sizes failed already: FSR 1, logged then
        }
        // Earlier frames' submissions read the shared images; nothing recorded since does.
        scheduler.WaitSubmitted();
        if (!metalfx->ConfigureSpatial(config.w, config.h, config.ow, config.oh, format)) {
            failed = config;
            return Unavailable(metalfx->Problem());
        }
        failed = {};
    }
    problem = nullptr;

    const auto input = metalfx->Color(), output = metalfx->Output();
    constexpr vk::ImageSubresourceRange color{vk::ImageAspectFlagBits::eColor, 0, 1, 0, 1};
    const auto cmdbuf = scheduler.CommandBuffer();
    // The shared images are rewritten whole: MetalFX was done with them before the last frame's
    // final submission could start, and that submission read the output before this one.
    const std::array<vk::ImageMemoryBarrier2, 3> before{{
        {
            .srcStageMask = vk::PipelineStageFlagBits2::eAllCommands,
            .srcAccessMask = vk::AccessFlagBits2::eMemoryWrite,
            .dstStageMask = vk::PipelineStageFlagBits2::eCopy,
            .dstAccessMask = vk::AccessFlagBits2::eTransferRead,
            .oldLayout = vk::ImageLayout::eShaderReadOnlyOptimal,
            .newLayout = vk::ImageLayout::eTransferSrcOptimal,
            .image = image,
            .subresourceRange = color,
        },
        {
            .srcStageMask = vk::PipelineStageFlagBits2::eAllCommands,
            .srcAccessMask = vk::AccessFlagBits2::eNone,
            .dstStageMask = vk::PipelineStageFlagBits2::eCopy,
            .dstAccessMask = vk::AccessFlagBits2::eTransferWrite,
            .oldLayout = vk::ImageLayout::eUndefined,
            .newLayout = vk::ImageLayout::eGeneral,
            .image = input.image,
            .subresourceRange = color,
        },
        // Metal writes the output after this submission.
        {
            .srcStageMask = vk::PipelineStageFlagBits2::eAllCommands,
            .srcAccessMask = vk::AccessFlagBits2::eNone,
            .dstStageMask = vk::PipelineStageFlagBits2::eAllCommands,
            .dstAccessMask = vk::AccessFlagBits2::eNone,
            .oldLayout = vk::ImageLayout::eUndefined,
            .newLayout = vk::ImageLayout::eGeneral,
            .image = output.image,
            .subresourceRange = color,
        },
    }};
    cmdbuf.pipelineBarrier2({
        .imageMemoryBarrierCount = u32(before.size()),
        .pImageMemoryBarriers = before.data(),
    });
    // The display image's bits as its view reads them (a raw copy between formats of one size).
    cmdbuf.copyImage(image, vk::ImageLayout::eTransferSrcOptimal, input.image,
                     vk::ImageLayout::eGeneral,
                     vk::ImageCopy{
                         .srcSubresource = {vk::ImageAspectFlagBits::eColor, 0, 0, 1},
                         .dstSubresource = {vk::ImageAspectFlagBits::eColor, 0, 0, 1},
                         .extent = {size.width, size.height, 1},
                     });
    // Back to the layout the presenter (and the texture cache) left the display image in.
    const vk::ImageMemoryBarrier2 restore{
        .srcStageMask = vk::PipelineStageFlagBits2::eCopy,
        .srcAccessMask = vk::AccessFlagBits2::eTransferRead,
        .dstStageMask = vk::PipelineStageFlagBits2::eAllCommands,
        .dstAccessMask = vk::AccessFlagBits2::eShaderRead,
        .oldLayout = vk::ImageLayout::eTransferSrcOptimal,
        .newLayout = vk::ImageLayout::eShaderReadOnlyOptimal,
        .image = image,
        .subresourceRange = color,
    };
    const vk::MemoryBarrier2 copied{
        .srcStageMask = vk::PipelineStageFlagBits2::eCopy,
        .srcAccessMask = vk::AccessFlagBits2::eTransferWrite,
        .dstStageMask = vk::PipelineStageFlagBits2::eAllCommands,
        .dstAccessMask = vk::AccessFlagBits2::eMemoryRead,
    };
    cmdbuf.pipelineBarrier2({
        .memoryBarrierCount = 1,
        .pMemoryBarriers = &copied,
        .imageMemoryBarrierCount = 1,
        .pImageMemoryBarriers = &restore,
    });
    // MetalFX starts once this submission has completed (its timeline tick).
    const u64 tick = scheduler.CurrentTick();
    scheduler.Flush();
    wait = metalfx->Submit(scheduler.GetWorkSemaphore()->Handle(), tick, {});
    // For the passes that read the result (after AddWait's wait).
    const vk::ImageMemoryBarrier2 upscaled{
        .srcStageMask = vk::PipelineStageFlagBits2::eAllCommands,
        .srcAccessMask = vk::AccessFlagBits2::eNone, // Metal's write: the wait covers it
        .dstStageMask = vk::PipelineStageFlagBits2::eAllCommands,
        .dstAccessMask = vk::AccessFlagBits2::eShaderRead,
        .oldLayout = vk::ImageLayout::eGeneral,
        .newLayout = vk::ImageLayout::eShaderReadOnlyOptimal,
        .image = output.image,
        .subresourceRange = color,
    };
    scheduler.CommandBuffer().pipelineBarrier2(
        {.imageMemoryBarrierCount = 1, .pImageMemoryBarriers = &upscaled});
    return output.view;
}

void MetalFxPass::AddWait(SubmitInfo& info) {
    if (!wait) {
        return;
    }
    info.AddWait(metalfx->Semaphore(), std::exchange(wait, 0),
                 vk::PipelineStageFlagBits::eAllCommands);
}

} // namespace Vulkan::HostPasses
