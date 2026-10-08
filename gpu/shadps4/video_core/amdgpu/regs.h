// SPDX-FileCopyrightText: Copyright 2025 shadPS4 Emulator Project
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include <algorithm>
#include <bitset>
#include <cstring>
#include <vector>
#include "video_core/amdgpu/regs_color.h"
#include "video_core/amdgpu/regs_depth.h"
#include "video_core/amdgpu/regs_primitive.h"
#include "video_core/amdgpu/regs_shader.h"
#include "video_core/amdgpu/regs_texture.h"
#include "video_core/amdgpu/regs_vertex.h"

namespace AmdGpu {

#define DO_CONCAT2(x, y) x##y
#define CONCAT2(x, y) DO_CONCAT2(x, y)
#define INSERT_PADDING_WORDS(num_words)                                                            \
    [[maybe_unused]] std::array<u32, num_words> CONCAT2(pad, __LINE__)

union Regs {
    static constexpr u32 NumRegs = 0xD000;
    static constexpr u32 UconfigRegWordOffset = 0xC000;
    static constexpr u32 ContextRegWordOffset = 0xA000;
    static constexpr u32 ConfigRegWordOffset = 0x2000;
    static constexpr u32 ShRegWordOffset = 0x2C00;

    struct {
        INSERT_PADDING_WORDS(11272);
        ShaderProgram ps_program;
        INSERT_PADDING_WORDS(44);
        ShaderProgram vs_program;
        INSERT_PADDING_WORDS(44);
        ShaderProgram gs_program;
        INSERT_PADDING_WORDS(44);
        ShaderProgram es_program;
        INSERT_PADDING_WORDS(44);
        ShaderProgram hs_program;
        INSERT_PADDING_WORDS(44);
        ShaderProgram ls_program;
        INSERT_PADDING_WORDS(164);
        ComputeProgram cs_program;
        INSERT_PADDING_WORDS(29104);
        DepthRenderControl depth_render_control;
        INSERT_PADDING_WORDS(1);
        DepthView depth_view;
        DepthRenderOverride depth_render_override;
        INSERT_PADDING_WORDS(1);
        Address depth_htile_data_base;
        INSERT_PADDING_WORDS(2);
        float depth_bounds_min;
        float depth_bounds_max;
        u32 stencil_clear;
        float depth_clear;
        Scissor screen_scissor;
        INSERT_PADDING_WORDS(2);
        DepthBuffer depth_buffer;
        INSERT_PADDING_WORDS(8);
        BorderColorBuffer ta_bc_base;
        INSERT_PADDING_WORDS(94);
        WindowOffset window_offset;
        ViewportScissor window_scissor;
        INSERT_PADDING_WORDS(11);
        ColorBufferMask color_target_mask;
        ColorBufferMask color_shader_mask;
        ViewportScissor generic_scissor;
        INSERT_PADDING_WORDS(2);
        std::array<ViewportScissor, NUM_VIEWPORTS> viewport_scissors;
        std::array<ViewportDepth, NUM_VIEWPORTS> viewport_depths;
        INSERT_PADDING_WORDS(46);
        u32 index_offset;
        u32 primitive_restart_index;
        INSERT_PADDING_WORDS(1);
        BlendConstants blend_constants;
        INSERT_PADDING_WORDS(2);
        StencilControl stencil_control;
        StencilRefMask stencil_ref_front;
        StencilRefMask stencil_ref_back;
        INSERT_PADDING_WORDS(1);
        std::array<ViewportBounds, NUM_VIEWPORTS> viewports;
        std::array<ClipUserData, NUM_CLIP_PLANES> clip_user_data;
        INSERT_PADDING_WORDS(10);
        std::array<PsInputControl, 32> ps_inputs;
        VsOutputConfig vs_output_config;
        INSERT_PADDING_WORDS(1);
        PsInput ps_input_ena;
        PsInput ps_input_addr;
        INSERT_PADDING_WORDS(1);
        u32 num_interp : 6;
        INSERT_PADDING_WORDS(1);
        BarycentricControl barycentric_control;
        INSERT_PADDING_WORDS(10);
        ShaderPosFormat shader_pos_format;
        ShaderExportFormat z_export_format;
        ColorExportFormat color_export_format;
        INSERT_PADDING_WORDS(26);
        std::array<BlendControl, NUM_COLOR_BUFFERS> blend_control;
        INSERT_PADDING_WORDS(17);
        IndexBufferBase index_base_address;
        INSERT_PADDING_WORDS(1);
        u32 draw_initiator;
        INSERT_PADDING_WORDS(3);
        DepthControl depth_control;
        INSERT_PADDING_WORDS(1);
        ColorControl color_control;
        DepthShaderControl depth_shader_control;
        ClipperControl clipper_control;
        PolygonControl polygon_control;
        ViewportControl viewport_control;
        VsOutputControl vs_output_control;
        INSERT_PADDING_WORDS(122);
        LineControl line_control;
        INSERT_PADDING_WORDS(4);
        TessFactorClamp hs_clamp;
        INSERT_PADDING_WORDS(7);
        GsMode vgt_gs_mode;
        GsOnchip vgt_gs_onchip_control;
        ModeControl mode_control;
        INSERT_PADDING_WORDS(5);
        RingOffset vgt_gsvs_ring_offset_1;
        RingOffset vgt_gsvs_ring_offset_2;
        RingOffset vgt_gsvs_ring_offset_3;
        GsOutPrimitiveType vgt_gs_out_prim_type;
        INSERT_PADDING_WORDS(1);
        u32 index_size;
        u32 max_index_size;
        IndexBufferType index_buffer_type;
        INSERT_PADDING_WORDS(1);
        u32 enable_primitive_id;
        INSERT_PADDING_WORDS(3);
        u32 enable_primitive_restart;
        INSERT_PADDING_WORDS(2);
        u32 vgt_instance_step_rate_0;
        u32 vgt_instance_step_rate_1;
        INSERT_PADDING_WORDS(1);
        u32 vgt_esgs_ring_itemsize;
        u32 vgt_gsvs_ring_itemsize;
        INSERT_PADDING_WORDS(33);
        u32 vgt_gs_max_vert_out : 11;
        INSERT_PADDING_WORDS(6);
        ShaderStageEnable stage_enable;
        LsHsConfig ls_hs_config;
        u32 vgt_gs_vert_itemsize[4];
        TessellationConfig tess_config;
        INSERT_PADDING_WORDS(3);
        PolygonOffset poly_offset;
        GsInstances vgt_gs_instance_cnt;
        StreamOutConfig vgt_strmout_config;
        StreamOutBufferConfig vgt_strmout_buffer_config;
        INSERT_PADDING_WORDS(17);
        AaConfig aa_config;
        INSERT_PADDING_WORDS(31);
        ColorBuffer color_buffers[NUM_COLOR_BUFFERS];
        INSERT_PADDING_WORDS(7343);
        StreamOutControl cp_strmout_cntl;
        INSERT_PADDING_WORDS(512);
        u32 vgt_esgs_ring_size;
        u32 vgt_gsvs_ring_size;
        PrimitiveType primitive_type;
        INSERT_PADDING_WORDS(9);
        u32 num_indices;
        VgtNumInstances num_instances;
        INSERT_PADDING_WORDS(2);
        TessFactorMemoryBase vgt_tf_memory_base;
    };
    std::array<u32, NumRegs> reg_array;

    const ShaderProgram* ProgramForStage(u32 index) const {
        switch (index) {
        case 0:
            return &ps_program;
        case 1:
            return &vs_program;
        case 2:
            return &gs_program;
        case 3:
            return &es_program;
        case 4:
            return &hs_program;
        case 5:
            return &ls_program;
        }
        return nullptr;
    }

    bool IsClipDisabled() const {
        return clipper_control.clip_disable || primitive_type == PrimitiveType::RectList;
    }

    void SetDefaults();
};

// gow3: register blocks written by a stretch of packets, and their values at its end. The
// draw-preparation scanner records one per submission so a worker reaches the state at the
// start of any later submission without replaying the packets in between.
struct RegDirty {
    static constexpr u32 BlockWords = 32;
    static constexpr u32 NumBlocks = Regs::NumRegs / BlockWords;
    std::bitset<NumBlocks> blocks;
    bool reset = false; ///< ClearState: defaults, then only the blocks marked after it

    void Mark(u32 word, u32 count) {
        if (count == 0 || word >= Regs::NumRegs) {
            return;
        }
        const u32 last = std::min(word + count, Regs::NumRegs) - 1;
        for (u32 block = word / BlockWords; block <= last / BlockWords; ++block) {
            blocks.set(block);
        }
    }
    template <typename T>
    void MarkField(const Regs& regs, const T& field) {
        const auto offset = reinterpret_cast<const u8*>(&field) -
                            reinterpret_cast<const u8*>(regs.reg_array.data());
        Mark(u32(offset / sizeof(u32)), u32((sizeof(T) + sizeof(u32) - 1) / sizeof(u32)));
    }
    void Clear() {
        blocks.reset();
        reset = false;
    }
};

struct RegDelta {
    bool reset = false;
    std::vector<u16> blocks;
    std::vector<u32> words;

    void Capture(const Regs& regs, const RegDirty& dirty) {
        reset = dirty.reset;
        blocks.clear();
        words.clear();
        for (u32 block = 0; block < RegDirty::NumBlocks; ++block) {
            if (dirty.blocks.test(block)) {
                blocks.push_back(u16(block));
                const u32* src = regs.reg_array.data() + block * RegDirty::BlockWords;
                words.insert(words.end(), src, src + RegDirty::BlockWords);
            }
        }
    }
    void Apply(Regs& regs) const {
        if (reset) {
            regs.SetDefaults();
        }
        for (size_t i = 0; i < blocks.size(); ++i) {
            std::memcpy(regs.reg_array.data() + blocks[i] * RegDirty::BlockWords,
                        words.data() + i * RegDirty::BlockWords,
                        RegDirty::BlockWords * sizeof(u32));
        }
    }
};

#undef DO_CONCAT2
#undef CONCAT2
#undef INSERT_PADDING_WORDS

} // namespace AmdGpu
