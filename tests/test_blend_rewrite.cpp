// SPDX-License-Identifier: GPL-2.0-or-later
// GNM scaled MIN/MAX blend rewrite (vk_blend_rewrite.h). No game, Vulkan device or window required.
#include <cassert>
#include <cstdio>

#include "video_core/renderer_vulkan/vk_blend_rewrite.h"

using namespace Vulkan;
using F = AmdGpu::BlendControl::BlendFactor;
using Fn = AmdGpu::BlendControl::BlendFunc;
using Nf = AmdGpu::NumberFormat;

static BlendRewriteResult Rewrite(BlendEquation& eq, Nf format = Nf::Unorm,
                                  BlendChannel channel = BlendChannel::Color) {
    return RewriteScaledMinMaxBlend(eq, format, channel);
}

int main() {
    // Not MIN/MAX, or MIN/MAX with unit factors: untouched.
    {
        BlendEquation eq{F::SrcAlpha, Fn::Add, F::OneMinusSrcAlpha};
        assert(Rewrite(eq) == BlendRewriteResult::Native);
        assert(eq.src_factor == F::SrcAlpha && eq.function == Fn::Add);
        BlendEquation mm{F::One, Fn::Min, F::One};
        assert(Rewrite(mm) == BlendRewriteResult::Native && mm.function == Fn::Min);
    }
    // MIN/MAX(s*s, d*d): native operation, squared by a second draw (God of War III shadows).
    {
        BlendEquation eq{F::SrcColor, Fn::Min, F::DstColor};
        assert(Rewrite(eq) == BlendRewriteResult::Squared && eq.function == Fn::Min);
        BlendEquation alpha{F::SrcAlpha, Fn::Max, F::DstAlpha};
        assert(Rewrite(alpha, Nf::Srgb, BlendChannel::Alpha) == BlendRewriteResult::Squared);
        // The color channel only squares through color factors.
        BlendEquation color{F::SrcAlpha, Fn::Max, F::DstAlpha};
        assert(Rewrite(color) == BlendRewriteResult::Unsupported);
    }
    // A zero factor: exact ADD. MIN(x, 0) = 0 and MAX(x, 0) = x on unsigned normalized targets.
    {
        BlendEquation min{F::One, Fn::Min, F::Zero};
        assert(Rewrite(min) == BlendRewriteResult::Exact);
        assert(min.function == Fn::Add && min.src_factor == F::Zero && min.dst_factor == F::Zero);
        BlendEquation max{F::Zero, Fn::Max, F::One};
        assert(Rewrite(max) == BlendRewriteResult::Exact);
        assert(max.function == Fn::Add && max.src_factor == F::Zero && max.dst_factor == F::One);
    }
    // Signed or float targets cannot use those identities.
    {
        BlendEquation eq{F::SrcColor, Fn::Min, F::DstColor};
        assert(Rewrite(eq, Nf::Float) == BlendRewriteResult::Unsupported);
        BlendEquation other{F::SrcAlpha, Fn::Min, F::OneMinusSrcAlpha};
        assert(Rewrite(other) == BlendRewriteResult::Unsupported);
    }
    std::puts("blend-rewrite-test: OK");
    return 0;
}
