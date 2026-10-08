// gow3: no ImGui overlay in the port; hooks used by the renderer are no-ops.
#pragma once
#include "video_core/renderer_vulkan/vk_common.h"
namespace Frontend { class WindowSDL; }
namespace Vulkan { class Instance; }
namespace ImGui::Core {
inline void Initialize(const Vulkan::Instance&, const Frontend::WindowSDL&, u32, vk::Format) {}
inline void OnSurfaceFormatChange(vk::Format) {}
inline void Shutdown(vk::Device) {}
inline bool MustKeepDrawing() { return false; }
struct TextureManager { static void Submit() {} };
} // namespace ImGui::Core
