// SPDX-License-Identifier: GPL-2.0-or-later
// gow3: in-game settings menu (Dear ImGui), drawn by the presenter into the swapchain image
// after the game frame, at display resolution. Insert (keyboard) or R3+L2 (gamepad) opens it;
// while it is open the game gets no pad/keyboard input. Settings live in gow3_settings.h.

#pragma once

#include <string>
#include <vector>
#include "common/types.h"
#include "video_core/renderer_vulkan/vk_common.h"

union SDL_Event;
struct SDL_Window;

namespace Vulkan {
class Instance;
}

namespace Gow3Overlay {

/// Present thread, once: the ImGui context and its Vulkan backend.
void Init(const Vulkan::Instance& instance, vk::Format format, u32 image_count);

/// Window thread, for every SDL event: true when the menu consumed it.
bool HandleEvent(const SDL_Event& event);
/// Turns SDL text input on while the menu edits a value (window thread, once per poll).
void UpdateTextInput(SDL_Window* window);

/// Start-up loading screen (pipeline cache warm-up): `total` 0 shows no count.
void SetLoading(bool active, u32 done, u32 total);
/// Background cache warm-up: a small corner note while the game runs; `total` 0 hides it.
void SetBackgroundLoading(u32 done, u32 total);

/// Whether anything is drawn this frame (menu open or FPS counter on).
bool Visible();

/// Present thread: draws into `view` (layout ColorAttachmentOptimal).
void Render(vk::CommandBuffer cmdbuf, vk::ImageView view, vk::Extent2D extent);

/// The menu is open or a text entry is shown: the game's input is held neutral.
bool CapturesInput();

/// Window thread: the game's text dialog (sceImeDialog). Shown as a box on screen until
/// `active` is false; typing goes to the window (sdl_window), the box only displays it.
void SetTextEntry(bool active, const std::string& prompt, const std::string& text);

/// The game's save list: shown until the player chooses (arrows/D-pad, Enter/Cross) or
/// cancels (Esc/Circle). PollChoice: -1 while open, -2 cancelled, else the chosen index.
void BeginChoice(const std::string& title, const std::vector<std::string>& items, int focus);
int PollChoice();

} // namespace Gow3Overlay
