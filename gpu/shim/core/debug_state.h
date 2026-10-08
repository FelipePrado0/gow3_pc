// gow3: shadPS4's frame/register dump tooling is not part of this port.
#pragma once
#include <span>
#include <string>
#include "common/types.h"
#include "shader_recompiler/runtime_info.h"
#include "video_core/amdgpu/regs.h"
#include "video_core/renderer_vulkan/vk_common.h"

namespace DebugStateType {
enum class QueueType { dcb = 0, ccb = 1, acb = 2 };
struct QueueDump {
    QueueType type;
    u32 submit_num;
    u32 num2;
    std::vector<u32> data;
    uintptr_t base_addr;
};
class DebugStateImpl {
public:
    using CsState = AmdGpu::ComputeProgram;
    bool is_using_fsr{};
    void IncFlipFrameNum() {}
    void IncDrawCall() noexcept {}
    void IncDispatch() noexcept {}
    void IncGnmFrameNum() {}
    u32 GetFrameNum() const { return 0; }
    bool DumpingCurrentFrame() const { return false; }
    bool DumpingCurrentReg() { return false; }
    bool ShouldPauseInSubmit() const { return false; }
    void PushQueueDump(QueueDump) {}
    void PushRegsDump(uintptr_t, uintptr_t, const AmdGpu::Regs&) {}
    void PushRegsDumpCompute(uintptr_t, uintptr_t, const CsState&) {}
    void CollectShader(const std::string&, Shader::SwStage, vk::ShaderModule, std::span<const u32>,
                       std::span<const u32>, std::span<const u32>, bool) {}
    void ShowDebugMessage(std::string) {}
    bool IsGuestThreadsPaused() const { return false; }
    void PauseGuestThreads() {}
    void ResumeGuestThreads() {}
};
} // namespace DebugStateType
inline DebugStateType::DebugStateImpl DebugStateInstance;
#define DebugState DebugStateInstance
