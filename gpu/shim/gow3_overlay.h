// SPDX-License-Identifier: GPL-2.0-or-later
// gow3: in-game settings menu (Dear ImGui), drawn by the presenter into the swapchain image
// after the game frame, at display resolution. Insert (keyboard) or R3+L2 (gamepad) opens it;
// while it is open the game gets no pad/keyboard input. Settings live in gow3_settings.h.

#pragma once

#include <string>
#include <vector>
#include "common/types.h"
#include "gow3_png.h"
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
/// Saved shaders read into memory at start (any thread): a lower-right note; `total` 0 hides it.
void SetShaderPrefetch(u32 done, u32 total);
/// GPU thread, when shader translations start or finish: `pending` still running, `started`
/// new since the last call. A lower-right note shows while any run and 2 s after.
void NoteShaderCompiles(u32 pending, u32 started);

/// Whether anything is drawn this frame (menu open or FPS counter on).
bool Visible();

/// Any thread: a short message in the lower right corner (a saved screenshot).
void ShowNotice(const std::string& text, int milliseconds = 3000);

/// Present thread: draws into `view` (layout ColorAttachmentOptimal).
void Render(vk::CommandBuffer cmdbuf, vk::ImageView view, vk::Extent2D extent);

/// The menu is open or a text entry is shown: the game's input is held neutral.
bool CapturesInput();

/// Window thread: the game's text dialog (sceImeDialog). Shown as a box on screen until
/// `active` is false; typing goes to the window (sdl_window), the box only displays it.
void SetTextEntry(bool active, const std::string& prompt, const std::string& text);

/// The game's save list: shown until the player chooses (arrows/D-pad, Enter/Cross) or
/// cancels (Esc/Circle). `pictures`: one per item (empty = none), drawn beside its text.
/// PollChoice: -1 while open, -2 cancelled, else the chosen index.
void BeginChoice(const std::string& title, const std::vector<std::string>& items,
                 std::vector<Gow3Png::Image> pictures, int focus);
int PollChoice();

} // namespace Gow3Overlay
