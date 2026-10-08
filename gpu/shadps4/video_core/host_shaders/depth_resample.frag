// SPDX-License-Identifier: GPL-2.0-or-later
// gow3: nearest resample of a depth/stencil image between native and reduced scene
// targets. Used where the format has no blit support (D32S8 on RADV).

#version 450 core
#extension GL_EXT_samplerless_texture_functions : require
#if defined(HAS_STENCIL) || defined(STENCIL_BITS)
#if defined(HAS_STENCIL)
#extension GL_ARB_shader_stencil_export : require
#endif
layout (binding = 1, set = 0) uniform utexture2D src_stencil;
#endif
#if defined(STENCIL_BITS)
layout (push_constant) uniform StencilBit { uint bit; } pc;
#endif

layout (binding = 0, set = 0) uniform texture2D src_depth;

layout (location = 0) in vec2 uv;

void main() {
    const ivec2 size = textureSize(src_depth, 0);
    const ivec2 coord = min(ivec2(uv * vec2(size)), size - 1);
#if defined(STENCIL_BITS)
    // Fixed-function stencil replacement writes one bit per draw. The first draw
    // (bit=0, writeMask=0) copies depth everywhere, including zero-stencil pixels.
    if (pc.bit != 0u && (texelFetch(src_stencil, coord, 0).r & pc.bit) == 0u) discard;
#endif
    gl_FragDepth = texelFetch(src_depth, coord, 0).r;
#if defined(HAS_STENCIL)
    gl_FragStencilRefARB = int(texelFetch(src_stencil, coord, 0).r);
#endif
}
