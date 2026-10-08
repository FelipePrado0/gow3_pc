// SPDX-License-Identifier: GPL-2.0-or-later
// bbport: integer attributes passed as smooth float varyings (instanced objects can pass the
// instance index this way, then truncate it with v_cvt_u32_f32). AMD hardware interpolates
// three equal values exactly; NVIDIA does not, and a value such as 0.99999994 truncates to 0 on
// part of the triangle: the object flickers with noise (coffins on the bridge, shadPS4 PR 1644).
// Turing and newer interpolate manually with VK_KHR_fragment_shader_barycentric; older GPUs
// (Pascal) lack it. There a truncation of an interpolated value is scaled by 1 + 2^-16 first: it
// restores the exact integer, and moves other values by far less than a pixel's gradient.

#include <cstdio>
#include "shader_recompiler/ir/basic_block.h"
#include "shader_recompiler/ir/ir_emitter.h"
#include "shader_recompiler/ir/passes/ir_passes.h"
#include "shader_recompiler/ir/program.h"

namespace Shader::Optimization {

namespace {
bool IsTruncation(IR::Opcode op) {
    switch (op) {
    case IR::Opcode::ConvertU32F32:
    case IR::Opcode::ConvertS32F32:
    case IR::Opcode::FPTrunc32:
    case IR::Opcode::FPFloor32:
        return true;
    default:
        return false;
    }
}

bool IsInterpolatedParam(const IR::Value& value) {
    if (value.IsImmediate()) {
        return false;
    }
    const IR::Inst* inst = value.TryInst();
    if (!inst) {
        return false;
    }
    return inst->GetOpcode() == IR::Opcode::GetAttribute && IR::IsParam(inst->Arg(0).Attribute());
}
} // namespace

void InterpolatedIntegerPass(IR::Program& program) {
    u32 patched = 0;
    for (IR::Block* const block : program.blocks) {
        for (IR::Inst& inst : block->Instructions()) {
            if (!IsTruncation(inst.GetOpcode()) || !IsInterpolatedParam(inst.Arg(0))) {
                continue;
            }
            IR::IREmitter ir{*block, IR::Block::InstructionList::s_iterator_to(inst)};
            const IR::F32 value{inst.Arg(0)};
            // value * (1 + 2^-16): away from zero, so truncation of either sign lands on the
            // integer the vertices held.
            const IR::F32 nudged{ir.FPMul(value, ir.Imm32(1.0f + 1.0f / 65536.0f))};
            inst.SetArg(0, nudged);
            ++patched;
        }
    }
    if (patched) {
        static u32 logged = 0;
        if (logged++ < 32) {
            std::printf("Shader: interpolated integer fix, %u truncations in %016llx\n", patched,
                        static_cast<unsigned long long>(program.info.pgm_hash));
        }
    }
}

} // namespace Shader::Optimization
