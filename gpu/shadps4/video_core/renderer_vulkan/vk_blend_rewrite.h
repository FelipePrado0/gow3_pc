// SPDX-FileCopyrightText: Copyright 2026 shadPS4 Emulator Project
// SPDX-License-Identifier: GPL-2.0-or-later
// gow3: from cuesta4/shadPS4 (eltutz), GNM scaled MIN/MAX blends.

#pragma once

#include "video_core/amdgpu/pixel_format.h"
#include "video_core/amdgpu/regs_color.h"

namespace Vulkan {

struct BlendEquation {
    AmdGpu::BlendControl::BlendFactor src_factor;
    AmdGpu::BlendControl::BlendFunc function;
    AmdGpu::BlendControl::BlendFactor dst_factor;
};

enum class BlendChannel {
    Color,
    Alpha,
};

enum class BlendRewriteResult {
    Native,
    Exact,
    /// MIN/MAX(src * src, dst * dst): the native MIN/MAX result must be squared by a second pass.
    Squared,
    Unsupported,
};

[[nodiscard]] constexpr bool IsUnsignedNormalizedBlendTarget(const AmdGpu::NumberFormat format) {
    using Format = AmdGpu::NumberFormat;
    return format == Format::Unorm || format == Format::Srgb;
}

/// Returns true when both operands of the equation are scaled by themselves. The alpha equation
/// reads the alpha component through color factors, so either form squares alpha.
[[nodiscard]] constexpr bool IsSelfScaledBlend(const BlendEquation& equation,
                                               const BlendChannel channel) {
    using Factor = AmdGpu::BlendControl::BlendFactor;
    if (channel == BlendChannel::Color) {
        return equation.src_factor == Factor::SrcColor && equation.dst_factor == Factor::DstColor;
    }
    return (equation.src_factor == Factor::SrcColor || equation.src_factor == Factor::SrcAlpha) &&
           (equation.dst_factor == Factor::DstColor || equation.dst_factor == Factor::DstAlpha);
}

/**
 * Vulkan ignores blend factors for MIN/MAX, while GNM applies them before the operation. If
 * either contribution is identically zero, the GNM equation can be represented exactly with ADD
 * on an unsigned normalized target: MIN(x, 0) clamps to zero and MAX(x, 0) clamps exactly as x.
 * If both operands are scaled by themselves, the values are non-negative on such a target, so
 * MIN(s * s, d * d) = MIN(s, d)^2 and MAX(s * s, d * d) = MAX(s, d)^2: the native operation is
 * kept and the caller squares its result with a second draw.
 */
[[nodiscard]] constexpr BlendRewriteResult RewriteScaledMinMaxBlend(
    BlendEquation& equation, const AmdGpu::NumberFormat target_format, const BlendChannel channel) {
    using Factor = AmdGpu::BlendControl::BlendFactor;
    using Function = AmdGpu::BlendControl::BlendFunc;

    if (equation.function != Function::Min && equation.function != Function::Max) {
        return BlendRewriteResult::Native;
    }
    if (equation.src_factor == Factor::One && equation.dst_factor == Factor::One) {
        return BlendRewriteResult::Native;
    }
    if (!IsUnsignedNormalizedBlendTarget(target_format)) {
        return BlendRewriteResult::Unsupported;
    }
    if (IsSelfScaledBlend(equation, channel)) {
        return BlendRewriteResult::Squared;
    }
    if (equation.src_factor != Factor::Zero && equation.dst_factor != Factor::Zero) {
        return BlendRewriteResult::Unsupported;
    }

    if (equation.function == Function::Min) {
        equation.src_factor = Factor::Zero;
        equation.dst_factor = Factor::Zero;
    }
    equation.function = Function::Add;
    return BlendRewriteResult::Exact;
}

} // namespace Vulkan
