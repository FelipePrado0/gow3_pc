// SPDX-License-Identifier: GPL-2.0-or-later
// Depth/stencil attachments that cannot affect a draw (vk_depth_stencil_state.h); adapted from
// cuesta4/shadPS4 (eltutz) tests/test_depth_stencil_state.cpp. No game or Vulkan device needed.
#include <cassert>
#include <cstdio>
#include <memory>

#include "video_core/renderer_vulkan/vk_depth_stencil_state.h"

using Vulkan::GetEffectiveDepthStencilState;

static std::unique_ptr<AmdGpu::Regs> DepthRegs() {
    auto regs = std::make_unique<AmdGpu::Regs>();
    regs->depth_buffer.z_read_base = 1;
    regs->depth_buffer.z_write_base = 1;
    regs->depth_buffer.z_info.format = AmdGpu::DepthBuffer::ZFormat::Z32Float;
    regs->depth_control.depth_enable = 1;
    regs->depth_control.depth_func = AmdGpu::CompareFunc::Always;
    return regs;
}

static void AddStencil(AmdGpu::Regs& regs) {
    regs.depth_buffer.stencil_read_base = 1;
    regs.depth_buffer.stencil_write_base = 1;
    regs.depth_buffer.stencil_info.format = AmdGpu::DepthBuffer::StencilFormat::Stencil8;
    regs.depth_control.stencil_enable = 1;
    regs.depth_control.stencil_ref_func = AmdGpu::CompareFunc::Always;
}

int main() {
    // Depth test that always passes and writes nothing: no attachment (it would crop the pass).
    {
        const auto state = GetEffectiveDepthStencilState(*DepthRegs());
        assert(!state.needs_attachment && !state.depth_test_enable && !state.depth_write_enable);
    }
    // Depth that can reject, write or bound fragments is kept.
    {
        auto regs = DepthRegs();
        regs->depth_control.depth_func = AmdGpu::CompareFunc::Less;
        assert(GetEffectiveDepthStencilState(*regs).needs_attachment);
        regs = DepthRegs();
        regs->depth_control.depth_write_enable = 1;
        const auto state = GetEffectiveDepthStencilState(*regs);
        assert(state.needs_attachment && state.depth_write_enable);
        regs = DepthRegs();
        regs->depth_control.depth_bounds_enable = 1;
        assert(GetEffectiveDepthStencilState(*regs).needs_attachment);
        regs = DepthRegs();
        regs->depth_control.disable_color_writes_on_depth_pass = 1;
        assert(GetEffectiveDepthStencilState(*regs).needs_attachment);
    }
    // Color written even where depth fails: the test cannot reject anything.
    {
        auto regs = DepthRegs();
        regs->depth_control.depth_func = AmdGpu::CompareFunc::Less;
        regs->depth_control.enable_color_writes_on_depth_fail = 1;
        assert(!GetEffectiveDepthStencilState(*regs).needs_attachment);
    }
    // Stencil: kept only when it can reject or write.
    {
        auto regs = DepthRegs();
        AddStencil(*regs);
        assert(!GetEffectiveDepthStencilState(*regs).needs_attachment);
        regs->depth_control.stencil_ref_func = AmdGpu::CompareFunc::Less;
        const auto state = GetEffectiveDepthStencilState(*regs);
        assert(state.needs_attachment && state.stencil_test_enable);
        regs = DepthRegs();
        AddStencil(*regs);
        regs->stencil_control.stencil_zpass_front = AmdGpu::StencilFunc::ReplaceTest;
        regs->stencil_ref_front.stencil_write_mask = 0xff;
        assert(GetEffectiveDepthStencilState(*regs).needs_attachment);
        regs->stencil_ref_front.stencil_write_mask = 0;
        assert(!GetEffectiveDepthStencilState(*regs).needs_attachment);
    }
    // HTILE maintenance needs the attachment only when it forces valid data.
    {
        auto regs = DepthRegs();
        regs->depth_render_control.resummarize_enable = 1;
        regs->depth_render_override.force_z_dirty = 1;
        assert(!GetEffectiveDepthStencilState(*regs).needs_attachment);
        regs->depth_render_override.force_z_valid = 1;
        assert(GetEffectiveDepthStencilState(*regs).needs_attachment);
    }
    std::puts("depth-stencil-state-test: OK");
    return 0;
}
