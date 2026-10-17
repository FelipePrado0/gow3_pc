// SPDX-License-Identifier: GPL-2.0-or-later
#include "gow3_overlay.h"

#include <algorithm>
#include <atomic>
#include <cfloat>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstdint>
#include <cstring>
#include <mutex>
#include <thread>
#include <string>
#include <vector>

#include <SDL3/SDL.h>
#include "gow3_settings.h"
#include "gow3_orbs.h"
#include "gow3_actors.h"
#include "gow3_graphics.h"
#include "gow3_perf_stats.h"
#include "../../src/actor_hook.h"

extern Gow3ActorSlot gow3_actor_slots[2];
extern "C" void runtime_restart(void); // src/probe.c: starts run.py again, ends this process
#include "common/elf_info.h"
#include "imgui.h"
#include "imgui_internal.h" // RegisterUserTexture (save pictures)
#include "../gow3gpu.h"       // gow3gpu_toast_trophy, gow3gpu_trophy_setup
#include "imgui_impl_vulkan.h"
#include "video_core/renderer_vulkan/vk_instance.h"
#include "video_core/renderer_vulkan/vk_scheduler.h"
#include "video_core/renderer_vulkan/vk_frame_capture.h"
#include "video_core/renderdoc.h"

// DejaVu Sans (Cyrillic), embedded (third_party/fonts, Bitstream Vera license).
#ifdef _WIN32
asm(".section .rdata,\"dr\"\n"
    ".balign 16\n"
    ".global gow3_font_ttf\n"
    "gow3_font_ttf:\n"
    ".incbin \"" GOW3_FONT_PATH "\"\n"
    ".global gow3_font_ttf_end\n"
    "gow3_font_ttf_end:\n"
    ".text\n");
#else
asm(".section .rodata\n"
    ".balign 16\n"
    ".hidden gow3_font_ttf\n"
    ".global gow3_font_ttf\n"
    "gow3_font_ttf:\n"
    ".incbin \"" GOW3_FONT_PATH "\"\n"
    ".hidden gow3_font_ttf_end\n"
    ".global gow3_font_ttf_end\n"
    "gow3_font_ttf_end:\n"
    ".previous\n");
#endif
extern "C" const unsigned char gow3_font_ttf[];
extern "C" const unsigned char gow3_font_ttf_end[];

namespace Gow3Overlay {

namespace {

std::mutex imgui_mutex; // the ImGui context: window thread (input) and present thread
bool initialized = false;
std::atomic<bool> menu_open{false};
bool r3_down = false, l2_down = false, r2_down = false;
std::atomic<bool> loading{false};
std::atomic<u32> loading_done{0}, loading_total{0};
std::atomic<u32> background_done{0}, background_total{0};
// A short message in a corner (saved screenshot), shown until notice_until.
std::mutex notice_mutex;
std::string notice_text;
std::atomic<s64> notice_until{0}; // steady_clock milliseconds
bool dirty = false; // settings changed while open: saved on close
int requested_tab = -1; // switch on the next frame (L1/R1, Q/E, menu opened)
// The game's text dialog (SetTextEntry), guarded by imgui_mutex.
bool text_entry_active = false;
std::string text_entry_prompt, text_entry_text;
// The game's save list (sceSaveDataDialog): a choice made with the arrows/D-pad.
std::atomic<bool> choice_active{false};
std::string choice_title;
std::vector<std::string> choice_items;
int choice_index = 0;
std::atomic<int> choice_result{-1}; // -1 open, -2 cancelled, else the chosen index
// Save pictures: decoded by BeginChoice, made textures on the present thread (null = none).
// Closed lists' textures wait in choice_retired until the backend has destroyed them.
std::vector<Gow3Png::Image> choice_pictures;
std::vector<ImTextureData*> choice_textures, choice_retired;
std::atomic<bool> choice_retiring{false};
float base_scale = 1.0f;

// Present rate for the FPS counter.
std::chrono::steady_clock::time_point last_present{};
float frame_ms_avg = 0.0f;

void SetOpen(bool value) {
    if (menu_open.exchange(value) == value) {
        return;
    }
    ImGui::GetIO().MouseDrawCursor = value;
    if (value) {
        requested_tab = Gow3Settings::Get().menu_tab;
    }
    if (!value && dirty) {
        dirty = false;
        Gow3Settings::Save();
    }
}

ImGuiKey KeyFromSdl(SDL_Keycode key) {
    switch (key) {
    case SDLK_TAB: return ImGuiKey_Tab;
    case SDLK_LEFT: return ImGuiKey_LeftArrow;
    case SDLK_RIGHT: return ImGuiKey_RightArrow;
    case SDLK_UP: return ImGuiKey_UpArrow;
    case SDLK_DOWN: return ImGuiKey_DownArrow;
    case SDLK_PAGEUP: return ImGuiKey_PageUp;
    case SDLK_PAGEDOWN: return ImGuiKey_PageDown;
    case SDLK_HOME: return ImGuiKey_Home;
    case SDLK_END: return ImGuiKey_End;
    case SDLK_DELETE: return ImGuiKey_Delete;
    case SDLK_BACKSPACE: return ImGuiKey_Backspace;
    case SDLK_SPACE: return ImGuiKey_Space;
    case SDLK_RETURN: return ImGuiKey_Enter;
    case SDLK_KP_ENTER: return ImGuiKey_KeypadEnter;
    case SDLK_ESCAPE: return ImGuiKey_Escape;
    case SDLK_Q: return ImGuiKey_Q;
    case SDLK_E: return ImGuiKey_E;
    case SDLK_LCTRL: return ImGuiKey_LeftCtrl;
    case SDLK_RCTRL: return ImGuiKey_RightCtrl;
    case SDLK_LSHIFT: return ImGuiKey_LeftShift;
    case SDLK_RSHIFT: return ImGuiKey_RightShift;
    case SDLK_LALT: return ImGuiKey_LeftAlt;
    case SDLK_RALT: return ImGuiKey_RightAlt;
    default: return ImGuiKey_None;
    }
}

ImGuiKey KeyFromGamepad(u8 button) {
    switch (button) {
    case SDL_GAMEPAD_BUTTON_SOUTH: return ImGuiKey_GamepadFaceDown;
    case SDL_GAMEPAD_BUTTON_EAST: return ImGuiKey_GamepadFaceRight;
    case SDL_GAMEPAD_BUTTON_WEST: return ImGuiKey_GamepadFaceLeft;
    case SDL_GAMEPAD_BUTTON_NORTH: return ImGuiKey_GamepadFaceUp;
    case SDL_GAMEPAD_BUTTON_DPAD_UP: return ImGuiKey_GamepadDpadUp;
    case SDL_GAMEPAD_BUTTON_DPAD_DOWN: return ImGuiKey_GamepadDpadDown;
    case SDL_GAMEPAD_BUTTON_DPAD_LEFT: return ImGuiKey_GamepadDpadLeft;
    case SDL_GAMEPAD_BUTTON_DPAD_RIGHT: return ImGuiKey_GamepadDpadRight;
    case SDL_GAMEPAD_BUTTON_LEFT_SHOULDER: return ImGuiKey_GamepadL1;
    case SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER: return ImGuiKey_GamepadR1;
    case SDL_GAMEPAD_BUTTON_START: return ImGuiKey_GamepadStart;
    case SDL_GAMEPAD_BUTTON_BACK: return ImGuiKey_GamepadBack;
    default: return ImGuiKey_None;
    }
}

float PixelDensity(SDL_WindowID id) {
    SDL_Window* window = SDL_GetWindowFromID(id);
    const float density = window ? SDL_GetWindowPixelDensity(window) : 1.0f;
    return density > 0.0f ? density : 1.0f;
}

// Marks the settings dirty when a widget changed them.
template <typename T>
void Store(std::atomic<T>& target, T value, bool changed) {
    if (changed) {
        target = value;
        dirty = true;
    }
}

void Checkbox(const char* label, std::atomic<bool>& value) {
    bool v = value;
    const bool changed = ImGui::Checkbox(label, &v);
    Store(value, v, changed);
}

// A combo over `count` labels storing an index or a value from `values`.
void Choice(const char* label, std::atomic<int>& target, const char* const* labels, const int* values,
            int count) {
    int current = 0;
    for (int i = 0; i < count; ++i) {
        if ((values ? values[i] : i) == target.load()) current = i;
    }
    if (ImGui::Combo(label, &current, labels, count)) {
        Store(target, values ? values[current] : current, true);
    }
}

// Controller button names in the help text follow GOW3_GLYPHS (run.py sets xbox or nintendo when
// the glyph feature is on); unset is PlayStation. Read once: the environment is fixed for a run.
static const char* PadText(const char* playstation, const char* xbox, const char* nintendo) {
    static const char* style = std::getenv("GOW3_GLYPHS");
    if (!style) return playstation;
    return !std::strcmp(style, "xbox") ? xbox : !std::strcmp(style, "nintendo") ? nintendo : playstation;
}

// The note for the item under the mouse or the gamepad/keyboard focus, shown in the footer.
const char* footer_help = nullptr;

void Help(const char* text) {
    if (text && (ImGui::IsItemHovered(ImGuiHoveredFlags_AllowWhenDisabled) || ImGui::IsItemFocused())) {
        footer_help = text;
    }
}

// Two-column rows: the name on the left, the control on the right.
bool BeginRows(const char* id) {
    if (!ImGui::BeginTable(id, 2, ImGuiTableFlags_SizingStretchProp)) {
        return false;
    }
    ImGui::TableSetupColumn("name", ImGuiTableColumnFlags_WidthStretch, 0.52f);
    ImGui::TableSetupColumn("value", ImGuiTableColumnFlags_WidthStretch, 0.48f);
    return true;
}

void Row(const char* label) {
    ImGui::TableNextRow();
    ImGui::TableNextColumn();
    ImGui::AlignTextToFramePadding();
    ImGui::TextUnformatted(label);
    ImGui::TableNextColumn();
    ImGui::SetNextItemWidth(-FLT_MIN);
    ImGui::PushID(label);
}

void CheckRow(const char* label, std::atomic<bool>& value, const char* help = nullptr) {
    Row(label);
    Checkbox("##value", value);
    Help(help);
    ImGui::PopID();
}

void ChoiceRow(const char* label, std::atomic<int>& target, const char* const* labels, const int* values,
               int count, const char* help = nullptr) {
    Row(label);
    Choice("##value", target, labels, values, count);
    Help(help);
    ImGui::PopID();
}

// A 0.1x to 100x multiplier with a 1x button.
void MultiplierRow(const char* label, std::atomic<float>& value, const char* help) {
    Row(label);
    const ImGuiStyle& style = ImGui::GetStyle();
    ImGui::SetNextItemWidth(-(ImGui::CalcTextSize("1x").x + style.FramePadding.x * 2 + style.ItemSpacing.x));
    float v = value;
    if (ImGui::SliderFloat("##value", &v, 0.1f, 100.0f, "%.2fx",
                           ImGuiSliderFlags_Logarithmic | ImGuiSliderFlags_AlwaysClamp)) {
        value = Gow3Orbs::ClampMultiplier(v);
        dirty = true;
    }
    Help(help);
    ImGui::SameLine();
    if (ImGui::SmallButton("1x")) {
        value = 1.0f;
        dirty = true;
    }
    Help(help);
    ImGui::PopID();
}

void InfoRow(const char* label, const char* text) {
    Row(label);
    ImGui::TextDisabled("%s", text);
    ImGui::PopID();
}

void SliderRow(const char* label, std::atomic<int>& value, int lo, int hi, const char* format, const char* help) {
    Row(label);
    int v = value;
    if (ImGui::SliderInt("##value", &v, lo, hi, format, ImGuiSliderFlags_AlwaysClamp)) {
        Store(value, std::clamp(v, lo, hi), true);
    }
    Help(help);
    ImGui::PopID();
}

const char* UpscalerLabel(int upscaler) {
    switch (upscaler) {
    case Gow3Settings::UpscalerFsr3: return "FSR 3.1";
    case Gow3Settings::UpscalerFsr4: return "FSR 4";
    case Gow3Settings::UpscalerFsr411: return "FSR 4.1.1";
    case Gow3Settings::UpscalerTaa: return "TAA";
    case Gow3Settings::UpscalerDlss: return "DLSS";
    default: return "";
    }
}

const Vulkan::Instance* overlay_instance = nullptr; // for the overlay's measurements

// The performance overlay's text with the current settings (also the menu's preview).
std::string OverlayText() {
    namespace P = Gow3PerfStats;
    const auto& s = Gow3Settings::Get();
    const unsigned items = s.overlay_items;
    if ((items & (P::Gpu | P::Cpu | P::Ram | P::Vram)) && overlay_instance) {
        P::Start(*overlay_instance);
    }
    P::Sample sample = P::Latest();
    sample.fps = frame_ms_avg > 0.0f ? 1000.0f / frame_ms_avg : 0.0f;
    sample.frame_ms = frame_ms_avg;
    sample.upscaler = UpscalerLabel(s.upscaler);
    return P::Format(sample, items, s.overlay_layout == 0);
}

constexpr const char* Unsupported = "Unavailable: game version or code signature mismatch.";

void GameTab() {
    auto& s = Gow3Settings::Get();
    ImGui::SeparatorText("Cheats");
    if (BeginRows("##cheats")) {
        static const char* const names[] = {"Infinite health", "Infinite magic", "Infinite item meter",
                                            "Infinite Rage of Sparta", "Max red orbs",
                                            "Unlock Very Hard", "Infinite Double Jumps"};
        static const char* const helps[] = {
            "Health stays full. Only Kratos is protected. Turning it off does not undo past refills.",
            "Magic stays full.", "The item meter (bow, Helios' head, boots) stays full.",
            "Rage of Sparta stays full.", "Red orbs stay at the maximum.",
            "Enable before opening New Game. Does not change an existing save difficulty.",
            "Repeat air jumps without landing. Turn off for normal movement."};
        for (unsigned cheat = 0; cheat < GOW3_CHEAT_COUNT; ++cheat) {
            const bool available = (s.cheats_supported.load() & (1u << cheat)) != 0;
            ImGui::BeginDisabled(!available);
            CheckRow(names[cheat], s.cheats[cheat], available ? helps[cheat] : Unsupported);
            ImGui::EndDisabled();
        }
        ImGui::EndTable();
    }

    ImGui::SeparatorText("Controller");
    if (BeginRows("##controller")) {
        SliderRow("Vibration", s.vibration, 0, 100, "%d%%",
                  "Strength of the controller's vibration. 0% turns it off.");
        ImGui::EndTable();
    }

    ImGui::SeparatorText("Multipliers");
    if (ImGui::Button("Reset all to 1x")) {
        for (auto* m : {&s.red_orb_multiplier, &s.green_orb_multiplier, &s.blue_orb_multiplier,
                        &s.gold_orb_multiplier, &s.damage_dealt, &s.damage_taken}) {
            *m = 1.0f;
        }
        dirty = true;
    }
    Help("Every multiplier back to 1x (the game's normal values).");
    if (BeginRows("##multipliers")) {
        const bool max_orbs = (s.cheats_supported.load() & (1u << 4)) && s.cheats[4].load();
        ImGui::BeginDisabled(!s.red_orbs_supported.load() || max_orbs);
        MultiplierRow("Red orbs", s.red_orb_multiplier,
                      !s.red_orbs_supported.load() ? Unsupported
                      : max_orbs ? "Turn off Max red orbs to use the multiplier."
                                 : "Red orbs gained. Orbs you have and prices stay the same.");
        ImGui::EndDisabled();
        ImGui::BeginDisabled(!s.orb_pickup_supported.load());
        const char* pickup = s.orb_pickup_supported.load() ? nullptr : Unsupported;
        MultiplierRow("Green orbs (health)", s.green_orb_multiplier,
                      pickup ? pickup : "Health from each green orb. The bar stays within its maximum.");
        MultiplierRow("Blue orbs (magic)", s.blue_orb_multiplier,
                      pickup ? pickup : "Magic from each blue orb. The bar stays within its maximum.");
        MultiplierRow("Gold orbs (Rage of Sparta)", s.gold_orb_multiplier,
                      pickup ? pickup : "Rage of Sparta from each gold orb. The bar stays within its maximum.");
        ImGui::EndDisabled();
        ImGui::BeginDisabled(!s.damage_supported.load());
        const char* damage = s.damage_supported.load() ? nullptr : Unsupported;
        MultiplierRow("Damage dealt", s.damage_dealt, damage ? damage : "Damage Kratos does to enemies.");
        MultiplierRow("Damage taken", s.damage_taken, damage ? damage : "Damage Kratos takes.");
        ImGui::EndDisabled();
        ImGui::EndTable();
    }

    ImGui::SeparatorText("HUD");
    if (BeginRows("##hud")) {
        ImGui::BeginDisabled(!s.actor_watch_supported.load());
        CheckRow("Enemy health bar", s.enemy_health_bar,
                 s.actor_watch_supported.load() ? "A thin bar at the top for the last enemy hit." : Unsupported);
        ImGui::EndDisabled();
        ImGui::EndTable();
    }
}

void ImageTab() {
    namespace G = Gow3Graphics;
    auto& s = Gow3Settings::Get();
    ImGui::SeparatorText("Display");
    if (BeginRows("##display")) {
        ChoiceRow("Display mode", s.display_mode, G::DisplayModeNames.data(), nullptr, G::DisplayModeCount,
                  "Windowed, borderless window or exclusive fullscreen.");
        CheckRow("VSync", s.vsync, "Waits for the monitor: no tearing, a little more input delay.");
        ImGui::EndTable();
    }
    const int iw = s.image_width, ih = s.image_height, ww = s.window_width, wh = s.window_height;
    const bool downscaling = iw > ww || ih > wh;
    const bool rcas_blocked = s.rcas && !s.rcas_applied;
    ImGui::SeparatorText("Sharpening");
    if (BeginRows("##scaling")) {
        ImGui::BeginDisabled(rcas_blocked);
        CheckRow("Sharpening (RCAS)", s.rcas,
                 rcas_blocked && downscaling ? "No effect: the game's image is larger than the window."
                 : rcas_blocked              ? "No effect: the game's image is smaller than the window."
                                             : "Sharpens the final image.");
        ImGui::EndDisabled();
        ImGui::BeginDisabled(!s.rcas || rcas_blocked);
        SliderRow("Sharpening strength", s.rcas_strength, 0, 100, "%d%%", "How strong the sharpening is.");
        ImGui::EndDisabled();
        ImGui::EndTable();
    }
    ImGui::SeparatorText("Textures (applied on restart)");
    if (BeginRows("##textures")) {
        static const char* const anisotropy_labels[] = {"Game default", "2x", "4x", "8x", "16x"};
        ChoiceRow("Anisotropic filtering", s.anisotropy, anisotropy_labels, G::Anisotropies.data(),
                  int(G::Anisotropies.size()),
                  "Sharper floors and walls seen at an angle. Little cost on most GPUs.");
        ImGui::EndTable();
    }
    ImGui::TextDisabled("F12 saves a screenshot to the user\\screenshots folder.");
    ImGui::SeparatorText("Information");
    if (BeginRows("##image_info")) {
        char text[96];
        std::snprintf(text, sizeof(text), "%d x %d", iw, ih);
        InfoRow("Game image", text);
        std::snprintf(text, sizeof(text), "%d x %d", ww, wh);
        InfoRow("Window", text);
        InfoRow("Sharpening", s.rcas_applied ? "applied" : "not applied");
        ImGui::EndTable();
    }
}

void PerformanceTab() {
    namespace G = Gow3Graphics;
    auto& s = Gow3Settings::Get();
    ImGui::SeparatorText("Live");
    if (BeginRows("##live")) {
        static const char* const fps_labels[] = {"30", "60", "120", "240", "Unlimited"};
        ChoiceRow("Frame rate limit", s.fps_limit, fps_labels, G::FpsLimits.data(), int(G::FpsLimits.size()),
                  "Caps the frame rate. The engine frame rate (below) is the highest it can go.");
        const int effective = G::EffectiveFps(s.fps_limit, s.applied_engine_fps);
        if (s.fps_limit == 0 || effective < s.fps_limit) {
            char text[48];
            std::snprintf(text, sizeof(text), "%d FPS (engine frame rate)", effective);
            InfoRow("Effective limit", text);
        }
        if (s.applied_engine_fps < s.startup_engine_fps) {
            InfoRow("Engine frame rate", "60 FPS: the frame rate patch is not available for this game build");
        }
        static const char* const queued_labels[] = {"1 (lowest latency)", "2", "Unlimited"};
        ChoiceRow("Frames queued", s.frames_queued, queued_labels, G::FramesQueued.data(),
                  int(G::FramesQueued.size()), "More frames queued: steadier FPS, slower response to input.");
        CheckRow("Compile new shaders in the background", s.async_shaders,
                 "Fewer stutters in new areas; an object or effect may appear a moment later.");
        ImGui::EndTable();
    }
    ImGui::SeparatorText("Applied on restart");
    if (BeginRows("##restart")) {
        // The built-in sizes, then those of the enabled mods' patch files (ultrawide, for one).
        // The same labels as the launcher (graphics_options.resolution_label): name, exact size, aspect ratio.
        static const char* const builtin_labels[] = {
            "Native · 1920 × 1080 · 16:9", "480p · 854 × 480 · 16:9",
            "720p · 1280 × 720 · 16:9", "1440p · 2560 × 1440 · 16:9",
            "1800p · 3200 × 1800 · 16:9", "4K · 3840 × 2160 · 16:9",
            "2560 × 1080 · 21:9 (beta)", "3440 × 1440 · 21:9 (beta)",
            "3840 × 1600 · 21:9 (beta)", "5120 × 1440 · 32:9 (beta)",
            "1920 × 1200 · 16:10 (beta)", "2560 × 1600 · 16:10 (beta)"};
        static_assert(std::size(builtin_labels) == G::Resolutions.size());
        const std::vector<std::string> resolution_names = G::ResolutionNames();
        std::vector<const char*> resolution_labels;
        for (std::size_t i = 0; i < resolution_names.size(); ++i) {
            resolution_labels.push_back(i < G::Resolutions.size() ? builtin_labels[i] : resolution_names[i].c_str());
        }
        ChoiceRow("Render resolution", s.render_resolution, resolution_labels.data(), nullptr,
                  int(resolution_labels.size()), "The resolution the game draws at. Higher is sharper and slower.");
        static const char* const engine_labels[] = {"60 (original)", "120", "240"};
        ChoiceRow("Engine frame rate", s.engine_fps, engine_labels, G::EngineFps.data(), int(G::EngineFps.size()),
                  "The highest frame rate the game itself runs at.");
        CheckRow("Game reads GPU data without waiting", s.stale_readback,
                 "Big FPS gain. Switch it off if lighting or objects look wrong.");
        CheckRow("Deferred GPU readbacks", s.deferred_readback,
                 "Fewer waits for the GPU. Switch it off if textures or shadows look wrong.");
        ImGui::EndTable();
    }
}

void OverlayTab() {
    namespace P = Gow3PerfStats;
    auto& s = Gow3Settings::Get();
    if (BeginRows("##overlay_on")) {
        CheckRow("Show overlay", s.show_fps, "The performance overlay in a corner of the screen.");
        ImGui::EndTable();
    }
    ImGui::BeginDisabled(!s.show_fps);
    ImGui::SeparatorText("What to show");
    if (ImGui::BeginTable("##overlay_items", 2, ImGuiTableFlags_SizingStretchProp)) {
        ImGui::TableNextColumn();
        static const char* const names[] = {"FPS", "Frametime", "Upscaler", "GPU (name and use)", "CPU (game)",
                                            "RAM (game / total)", "VRAM (used / available)"};
        for (unsigned item = 0; item < 7; ++item) {
            const unsigned bit = 1u << item;
            bool on = (s.overlay_items & bit) != 0;
            if (ImGui::Checkbox(names[item], &on)) {
                s.overlay_items = on ? (s.overlay_items | bit) : (s.overlay_items & ~bit);
                dirty = true;
            }
        }
        ImGui::TableNextColumn();
        ImGui::TextDisabled("Preview");
        ImGui::BeginChild("##preview", ImVec2(0, 0), ImGuiChildFlags_Borders | ImGuiChildFlags_AutoResizeY);
        ImGui::SetWindowFontScale(s.overlay_scale / 100.0f);
        const std::string text = OverlayText();
        ImGui::TextUnformatted(text.empty() ? "(nothing selected)" : text.c_str());
        ImGui::SetWindowFontScale(1.0f);
        ImGui::EndChild();
        ImGui::EndTable();
    }
    ImGui::SeparatorText("Appearance");
    if (BeginRows("##appearance")) {
        SliderRow("Font size", s.overlay_scale, 50, 300, "%d%%", "Size of the overlay text.");
        static const char* const corners[] = {"Top left", "Top right", "Bottom left", "Bottom right"};
        ChoiceRow("Corner", s.overlay_corner, corners, nullptr, 4, "Where the overlay sits.");
        SliderRow("Background opacity", s.overlay_opacity, 0, 100, "%d%%", "0% is no background.");
        static const char* const layouts[] = {"One line", "One item per line"};
        ChoiceRow("Layout", s.overlay_layout, layouts, nullptr, 2, "Items side by side or stacked.");
        ImGui::EndTable();
    }
    ImGui::EndDisabled();
    ImGui::SeparatorText("About");
    if (BeginRows("##about")) {
        const auto& elf = Common::ElfInfo::Instance();
        char text[160];
        std::snprintf(text, sizeof(text), "%.*s v%.*s", int(elf.GameSerial().size()), elf.GameSerial().data(),
                      int(elf.AppVer().size()), elf.AppVer().data());
        InfoRow("Game", text);
        const u32 total = background_total;
        if (total) {
            std::snprintf(text, sizeof(text), "loading %u%%", u32(u64(background_done) * 100 / total));
        } else {
            std::snprintf(text, sizeof(text), "ready");
        }
        InfoRow("Shader cache", text);
        std::string hooks;
        const auto add = [&](bool on, const char* name) {
            if (on) {
                hooks += hooks.empty() ? name : std::string(", ") + name;
            }
        };
        add(s.cheats_supported.load() != 0, "cheats");
        add(s.red_orbs_supported.load(), "red orbs");
        add(s.orb_pickup_supported.load(), "orb pickups");
        add(s.damage_supported.load(), "damage");
        add(s.actor_watch_supported.load(), "enemy health");
        InfoRow("Game hooks", hooks.empty() ? "none" : hooks.c_str());
        ImGui::EndTable();
    }
}

// Startup options changed since this launch (applied by "Apply and restart").
int RestartChanges() {
    const auto& s = Gow3Settings::Get();
    return (s.render_resolution != s.startup_render_resolution) + (s.engine_fps != s.startup_engine_fps) +
           (s.deferred_readback != s.startup_deferred_readback) + (s.stale_readback != s.startup_stale_readback) +
           (s.anisotropy != s.startup_anisotropy);
}

void Menu() {
    auto& s = Gow3Settings::Get();
    const ImGuiViewport* viewport = ImGui::GetMainViewport();
    ImGui::SetNextWindowPos(ImVec2(viewport->WorkPos.x + viewport->WorkSize.x * 0.5f,
                                   viewport->WorkPos.y + viewport->WorkSize.y * 0.5f),
                            ImGuiCond_Appearing, ImVec2(0.5f, 0.5f));
    ImGui::SetNextWindowSize(ImVec2(std::min(760.0f * base_scale, viewport->WorkSize.x * 0.95f),
                                    std::min(680.0f * base_scale, viewport->WorkSize.y * 0.9f)),
                             ImGuiCond_Appearing);
    bool keep_open = true;
    // gow3: the game's own title (param.sfo), not a fixed one.
    static const std::string heading = [] {
        const std::string_view title = Common::ElfInfo::Instance().Title();
        return std::string(title.empty() ? "God of War III" : title) + PadText("  (Insert / R3+L2)", "  (Insert / RS+LT)", "  (Insert / RS+ZL)");
    }();
    if (!ImGui::Begin(heading.c_str(), &keep_open, ImGuiWindowFlags_NoCollapse)) {
        ImGui::End();
        return;
    }
    footer_help = nullptr;
    constexpr int TabCount = 4;
    const ImGuiIO& io = ImGui::GetIO();
    if (!io.WantTextInput) {
        const int step = ImGui::IsKeyPressed(ImGuiKey_GamepadR1, false) || ImGui::IsKeyPressed(ImGuiKey_E, false) ? 1
                         : ImGui::IsKeyPressed(ImGuiKey_GamepadL1, false) || ImGui::IsKeyPressed(ImGuiKey_Q, false)
                             ? -1
                             : 0;
        if (step) {
            requested_tab = (s.menu_tab + step + TabCount) % TabCount;
        }
    }
    const float footer = ImGui::GetFrameHeightWithSpacing() * 2.0f + ImGui::GetTextLineHeightWithSpacing() * 2.0f;
    if (ImGui::BeginTabBar("##tabs")) {
        static const char* const tabs[TabCount] = {"Game", "Image", "Performance", "Overlay"};
        for (int tab = 0; tab < TabCount; ++tab) {
            const ImGuiTabItemFlags flags = requested_tab == tab ? ImGuiTabItemFlags_SetSelected : 0;
            if (!ImGui::BeginTabItem(tabs[tab], nullptr, flags)) {
                continue;
            }
            if (s.menu_tab != tab && requested_tab < 0) {
                s.menu_tab = tab;
                dirty = true;
            }
            if (ImGui::BeginChild("##body", ImVec2(0, -footer))) {
                switch (tab) {
                case 0: GameTab(); break;
                case 1: ImageTab(); break;
                case 2: PerformanceTab(); break;
                default: OverlayTab(); break;
                }
            }
            ImGui::EndChild();
            ImGui::EndTabItem();
        }
        if (requested_tab >= 0) {
            s.menu_tab = requested_tab;
            dirty = true;
            requested_tab = -1;
        }
        ImGui::EndTabBar();
    }

    ImGui::Separator();
    ImGui::PushTextWrapPos(0.0f);
    ImGui::TextDisabled("%s", footer_help ? footer_help : PadText("L1 / R1 or Q / E: switch tabs. Settings are saved in gow3.ini.", "LB / RB or Q / E: switch tabs. Settings are saved in gow3.ini.", "L / R or Q / E: switch tabs. Settings are saved in gow3.ini."));
    ImGui::PopTextWrapPos();
    if (const int changes = RestartChanges()) {
        ImGui::TextColored(ImVec4(0.85f, 0.72f, 0.45f, 1.0f), "%d change%s need%s a restart (unsaved progress is lost).",
                           changes, changes == 1 ? "" : "s", changes == 1 ? "s" : "");
        ImGui::SameLine();
        ImGui::BeginDisabled(!std::getenv("GOW3_RESTART_COMMAND"));
        if (ImGui::Button("Apply and restart")) {
            // The new launch confirms the options once the game shows; run.py restores the
            // previous ones if it never does (restart_unconfirmed).
            s.restart_unconfirmed = true;
            Gow3Settings::Save();
            runtime_restart();
        }
        ImGui::EndDisabled();
    } else {
        ImGui::NewLine();
    }
    const float close_width = ImGui::CalcTextSize("Close").x + ImGui::GetStyle().FramePadding.x * 2;
    ImGui::SetCursorPosX(ImGui::GetCursorPosX() + ImGui::GetContentRegionAvail().x - close_width);
    if (ImGui::Button("Close")) {
        keep_open = false;
    }
    ImGui::End();
    if (!keep_open) {
        SetOpen(false);
    }
}

// Enemy health bar from the damage hook's records (gow3_actors.cpp).
// Only the copied values are read, never the game's memory: an actor may be gone by now.
void HealthDisplay() {
    struct Seen {
        uint32_t seq = 0;
        std::chrono::steady_clock::time_point at{};
    };
    static Seen seen[2];
    const auto& s = Gow3Settings::Get();
    if (!s.actor_watch_supported.load()) {
        return;
    }
    const auto now = std::chrono::steady_clock::now();
    Gow3ActorSlot slot[2];
    for (int i = 0; i < 2; ++i) {
        std::memcpy(&slot[i], const_cast<const Gow3ActorSlot*>(&gow3_actor_slots[i]), sizeof(slot[i]));
        if (slot[i].seq != seen[i].seq) {
            seen[i] = {slot[i].seq, now};
        }
    }
    const auto recent = [&](int i, int seconds) {
        return seen[i].seq != 0 && now - seen[i].at < std::chrono::seconds(seconds);
    };
    const ImGuiViewport* viewport = ImGui::GetMainViewport();
    ImDrawList* draw = ImGui::GetForegroundDrawList();
    const float scale = base_scale;
    if (s.enemy_health_bar && recent(1, 4) && Gow3Actors::Valid(slot[1].health, slot[1].max_health)) {
        // Minimal: a thin green bar centered on the screen's top edge; it shrinks toward the center
        // so it stays symmetric, with the health in numbers below.
        const ImVec2 display = ImGui::GetIO().DisplaySize;
        const float width = 260.0f * scale, height = 5.0f * scale, center = display.x * 0.5f;
        const float top = 36.0f * scale;
        const float fraction = Gow3Actors::Fraction(slot[1].health, slot[1].max_health);
        draw->AddRectFilled(ImVec2(center - width * 0.5f - scale, top - scale),
                            ImVec2(center + width * 0.5f + scale, top + height + scale),
                            IM_COL32(0, 0, 0, 140), 2 * scale);
        const float half = width * 0.5f * fraction;
        draw->AddRectFilled(ImVec2(center - half, top), ImVec2(center + half, top + height),
                            IM_COL32(70, 190, 80, 220), 2 * scale);
        char text[48];
        std::snprintf(text, sizeof(text), "%.0f / %.0f", std::ceil(slot[1].health), slot[1].max_health);
        const float font = ImGui::GetFontSize() * 0.9f;
        const ImVec2 size = ImGui::GetFont()->CalcTextSizeA(font, FLT_MAX, 0.0f, text);
        const ImVec2 at{center - size.x * 0.5f, top + height + 4.0f * scale};
        draw->AddText(ImGui::GetFont(), font, ImVec2(at.x + scale, at.y + scale), IM_COL32(0, 0, 0, 160), text);
        draw->AddText(ImGui::GetFont(), font, at, IM_COL32(225, 235, 220, 220), text);
    }
}

// The game's text dialog: what is typed, and how to finish (keyboard or controller).
void TextEntryBox() {
    const ImGuiViewport* viewport = ImGui::GetMainViewport();
    ImGui::SetNextWindowPos(ImVec2(viewport->WorkPos.x + viewport->WorkSize.x * 0.5f,
                                   viewport->WorkPos.y + viewport->WorkSize.y * 0.42f),
                            ImGuiCond_Always, ImVec2(0.5f, 0.5f));
    ImGui::SetNextWindowSizeConstraints(ImVec2(viewport->WorkSize.x * 0.32f, 0.0f),
                                        ImVec2(viewport->WorkSize.x * 0.8f, FLT_MAX));
    ImGui::SetNextWindowBgAlpha(0.92f);
    ImGui::Begin("##text_entry", nullptr,
                 ImGuiWindowFlags_NoDecoration | ImGuiWindowFlags_AlwaysAutoResize |
                     ImGuiWindowFlags_NoInputs | ImGuiWindowFlags_NoNav |
                     ImGuiWindowFlags_NoFocusOnAppearing);
    ImGui::TextColored(ImVec4(0.85f, 0.72f, 0.45f, 1.0f), "%s", text_entry_prompt.c_str());
    ImGui::Separator();
    ImGui::SetWindowFontScale(1.4f);
    ImGui::Text("%s_", text_entry_text.c_str());
    ImGui::SetWindowFontScale(1.0f);
    ImGui::Separator();
    ImGui::TextDisabled("Keyboard: type, Backspace to erase, Enter = OK, Esc = cancel");
    ImGui::TextDisabled("%s", PadText("Controller: Cross (A) = OK, Circle (B) = cancel", "Controller: A = OK, B = cancel", "Controller: B = OK, A = cancel"));
    ImGui::End();
}

void ChoiceBox() {
    const ImGuiViewport* viewport = ImGui::GetMainViewport();
    ImGui::SetNextWindowPos(ImVec2(viewport->WorkPos.x + viewport->WorkSize.x * 0.5f,
                                   viewport->WorkPos.y + viewport->WorkSize.y * 0.5f),
                            ImGuiCond_Always, ImVec2(0.5f, 0.5f));
    ImGui::SetNextWindowSizeConstraints(ImVec2(viewport->WorkSize.x * 0.36f, 0.0f),
                                        ImVec2(viewport->WorkSize.x * 0.8f, viewport->WorkSize.y * 0.8f));
    ImGui::SetNextWindowBgAlpha(0.94f);
    ImGui::Begin("##choice", nullptr,
                 ImGuiWindowFlags_NoDecoration | ImGuiWindowFlags_AlwaysAutoResize |
                     ImGuiWindowFlags_NoInputs | ImGuiWindowFlags_NoNav |
                     ImGuiWindowFlags_NoFocusOnAppearing);
    ImGui::TextColored(ImVec4(0.85f, 0.72f, 0.45f, 1.0f), "%s", choice_title.c_str());
    ImGui::Separator();
    // Save pictures are 228x128; rows without one keep the text aligned when others have one.
    const bool pictures = std::any_of(choice_textures.begin(), choice_textures.end(),
                                      [](ImTextureData* tex) { return tex != nullptr; });
    const ImVec2 picture_size(228.0f * base_scale, 128.0f * base_scale);
    for (int i = 0; i < int(choice_items.size()); ++i) {
        const bool selected = i == choice_index;
        ImGui::BeginGroup();
        ImTextureData* tex = i < int(choice_textures.size()) ? choice_textures[i] : nullptr;
        if (tex) {
            ImGui::Image(tex->GetTexRef(), picture_size);
        } else if (pictures) {
            ImGui::Dummy(picture_size);
        }
        if (pictures) {
            ImGui::SameLine();
        }
        if (selected) {
            ImGui::TextColored(ImVec4(1.0f, 0.9f, 0.55f, 1.0f), "> %s", choice_items[i].c_str());
        } else {
            ImGui::Text("  %s", choice_items[i].c_str());
        }
        ImGui::EndGroup();
        if (selected) {
            if (pictures) {
                const ImVec2 min = ImGui::GetItemRectMin(), max = ImGui::GetItemRectMax();
                const float pad = 3.0f * base_scale;
                ImGui::GetWindowDrawList()->AddRect(ImVec2(min.x - pad, min.y - pad),
                                                    ImVec2(max.x + pad, max.y + pad),
                                                    IM_COL32(255, 230, 140, 255), 0.0f, 0,
                                                    2.0f * base_scale);
            }
            ImGui::SetScrollHereY();
        }
    }
    ImGui::Separator();
    ImGui::TextDisabled("%s", PadText("Up/Down or D-pad: choose   Enter or Cross (A): OK   Esc or Circle (B): cancel", "Up/Down or D-pad: choose   Enter or A: OK   Esc or B: cancel", "Up/Down or D-pad: choose   Enter or B: OK   Esc or A: cancel"));
    ImGui::End();
}

void LoadingScreen() {
    const ImGuiViewport* viewport = ImGui::GetMainViewport();
    ImGui::SetNextWindowPos(ImVec2(viewport->WorkPos.x + viewport->WorkSize.x * 0.5f,
                                   viewport->WorkPos.y + viewport->WorkSize.y * 0.5f),
                            ImGuiCond_Always, ImVec2(0.5f, 0.5f));
    ImGui::SetNextWindowSize(ImVec2(viewport->WorkSize.x * 0.4f, 0.0f), ImGuiCond_Always);
    ImGui::SetNextWindowBgAlpha(0.0f);
    ImGui::Begin("##loading", nullptr,
                 ImGuiWindowFlags_NoDecoration | ImGuiWindowFlags_NoInputs |
                     ImGuiWindowFlags_NoNav | ImGuiWindowFlags_NoFocusOnAppearing);
    const u32 done = loading_done, total = loading_total;
    ImGui::SetWindowFontScale(1.6f);
    ImGui::TextUnformatted(total ? "Loading shader cache..." : "Preparing graphics...");
    ImGui::SetWindowFontScale(1.0f);
    if (total) {
        char count[64];
        std::snprintf(count, sizeof(count), "%u / %u (%u%%)", done, total,
                      u32(u64(done) * 100 / total));
        ImGui::ProgressBar(float(done) / float(total), ImVec2(-1.0f, 0.0f), count);
    }
    ImGui::TextDisabled("The first start after an update takes longer.");
    ImGui::End();
}

void FpsCounter() {
    const auto& s = Gow3Settings::Get();
    const std::string text = OverlayText();
    if (text.empty()) {
        return;
    }
    const ImGuiViewport* viewport = ImGui::GetMainViewport();
    const float pad = 12.0f * base_scale;
    const int corner = s.overlay_corner;
    const bool right = corner == 1 || corner == 3, bottom = corner >= 2;
    ImGui::SetNextWindowPos(ImVec2(right ? viewport->WorkPos.x + viewport->WorkSize.x - pad : viewport->WorkPos.x + pad,
                                   bottom ? viewport->WorkPos.y + viewport->WorkSize.y - pad : viewport->WorkPos.y + pad),
                            ImGuiCond_Always, ImVec2(right ? 1.0f : 0.0f, bottom ? 1.0f : 0.0f));
    ImGui::SetNextWindowBgAlpha(s.overlay_opacity / 100.0f);
    ImGui::Begin("##fps", nullptr,
                 ImGuiWindowFlags_NoDecoration | ImGuiWindowFlags_AlwaysAutoResize |
                     ImGuiWindowFlags_NoInputs | ImGuiWindowFlags_NoNav |
                     ImGuiWindowFlags_NoFocusOnAppearing);
    ImGui::SetWindowFontScale(s.overlay_scale / 100.0f);
    ImGui::TextUnformatted(text.c_str());
    ImGui::End();
}

void BackgroundLoading() {
    const u32 done = background_done, total = std::max(background_total.load(), 1u);
    const ImGuiViewport* viewport = ImGui::GetMainViewport();
    const float pad = 12.0f * base_scale;
    ImGui::SetNextWindowPos(ImVec2(viewport->WorkPos.x + pad,
                                   viewport->WorkPos.y + viewport->WorkSize.y - pad),
                            ImGuiCond_Always, ImVec2(0.0f, 1.0f));
    ImGui::SetNextWindowBgAlpha(0.35f);
    ImGui::Begin("##background_loading", nullptr,
                 ImGuiWindowFlags_NoDecoration | ImGuiWindowFlags_AlwaysAutoResize |
                     ImGuiWindowFlags_NoInputs | ImGuiWindowFlags_NoNav |
                     ImGuiWindowFlags_NoFocusOnAppearing);
    ImGui::TextDisabled("Loading shader cache %u%%", u32(u64(done) * 100 / total));
    ImGui::End();
}

// Trophy texts and list (set once by gow3gpu_trophy_setup from the runtime, which reads them from the
// launcher's trophy_texts.txt): the labels of the notice and of the in-game trophy window.
struct TrophyLabels {
    std::string heading = "Trophies", notice = "TROPHY UNLOCKED", unlocked = "UNLOCKED", locked = "LOCKED";
    std::string all = "All trophies", only_unlocked = "Unlocked", only_locked = "Locked";
    std::string counter = "{} of {} unlocked";
    std::string hint = "Up/Down: scroll   Left/Right: filter   F8 / R3+R2 / Esc: close";
};
struct TrophyRow {
    int id = 0;
    std::string name, detail;
    Gow3Png::Image image;         // decoded icon, waiting for its texture
    ImTextureData* tex = nullptr; // render thread, lives for the whole run
};
TrophyLabels trophy_labels;                      // imgui_mutex
std::vector<TrophyRow> trophy_rows;              // imgui_mutex
const volatile uint32_t* trophy_flags = nullptr; // the runtime's unlocked bits, read live
std::atomic<bool> trophy_ready{false}, trophy_started{false};
std::atomic<bool> trophy_view{false};            // the in-game trophy window is open
int trophy_filter = 0;                           // 0 all, 1 unlocked, 2 locked
float trophy_scroll = 0.0f;                      // pixels

// Trophy notices: queued by gow3gpu_toast/gow3gpu_toast_icon (any thread), drawn by Render in the
// top-right corner, below the FPS counter.
struct Toast {
    std::string text, detail;     // title (the trophy's name) and the line under it (its description)
    std::chrono::steady_clock::time_point start, until;
    Gow3Png::Image image;         // decoded icon, waiting for its texture (empty: none)
    ImTextureData* tex = nullptr; // render thread only
};
std::mutex toast_mutex;
std::vector<Toast> toasts;
std::vector<ImTextureData*> toast_retired; // render thread only; freed once the backend destroyed them
std::atomic<bool> toast_active{false};
constexpr float kToastSeconds = 5.0f;

// imgui_mutex held, before NewFrame: icons that arrived become textures, destroyed ones are freed.
void UpdateToastTextures() {
    std::scoped_lock lock{toast_mutex};
    for (Toast& t : toasts) {
        if (t.tex || t.image.rgba.empty()) {
            continue;
        }
        ImTextureData* tex = IM_NEW(ImTextureData)();
        tex->Create(ImTextureFormat_RGBA32, t.image.width, t.image.height);
        std::memcpy(tex->GetPixels(), t.image.rgba.data(), t.image.rgba.size());
        ImGui::RegisterUserTexture(tex);
        t.tex = tex;
        t.image = {};
    }
    std::erase_if(toast_retired, [](ImTextureData* tex) {
        if (tex->Status != ImTextureStatus_Destroyed) {
            return false;
        }
        ImGui::UnregisterUserTexture(tex);
        IM_DELETE(tex);
        return true;
    });
    toast_active = !toasts.empty() || !toast_retired.empty();
}

// A gold medal with a cup, drawn with shapes: used when a trophy has no icon. a: overall opacity.
void DrawTrophyIcon(ImDrawList* dl, ImVec2 c, float r, float a) {
    auto col = [&](float R, float G, float B, float A) {
        return ImGui::ColorConvertFloat4ToU32(ImVec4(R, G, B, A * a));
    };
    dl->AddCircleFilled(c, r, col(0.96f, 0.74f, 0.20f, 1.0f), 48);
    dl->AddCircle(c, r, col(1.0f, 0.92f, 0.60f, 1.0f), 48, std::max(1.5f, r * 0.07f));
    const ImU32 ink = col(0.12f, 0.08f, 0.02f, 1.0f);
    const float u = r / 10.0f;
    dl->AddCircle(ImVec2(c.x - 5.4f * u, c.y - 2.6f * u), 2.2f * u, ink, 20, 1.3f * u);
    dl->AddCircle(ImVec2(c.x + 5.4f * u, c.y - 2.6f * u), 2.2f * u, ink, 20, 1.3f * u);
    dl->AddQuadFilled(ImVec2(c.x - 4.6f * u, c.y - 5.8f * u), ImVec2(c.x + 4.6f * u, c.y - 5.8f * u),
                      ImVec2(c.x + 2.4f * u, c.y + 1.6f * u), ImVec2(c.x - 2.4f * u, c.y + 1.6f * u), ink);
    dl->AddRectFilled(ImVec2(c.x - 0.9f * u, c.y + 1.6f * u), ImVec2(c.x + 0.9f * u, c.y + 4.4f * u), ink);
    dl->AddRectFilled(ImVec2(c.x - 3.2f * u, c.y + 4.4f * u), ImVec2(c.x + 3.2f * u, c.y + 6.0f * u), ink, 0.5f * u);
}

void ToastBox() {
    const auto now = std::chrono::steady_clock::now();
    struct Line {
        std::string text, detail;
        float elapsed, left;
        ImTextureData* tex;
    };
    std::vector<Line> lines;
    {
        std::scoped_lock lock{toast_mutex};
        for (Toast& t : toasts) {
            if (t.until <= now && t.tex) {
                t.tex->WantDestroyNextFrame = true;
                toast_retired.push_back(t.tex);
                t.tex = nullptr;
            }
        }
        toasts.erase(std::remove_if(toasts.begin(), toasts.end(),
                                    [&](const Toast& t) { return t.until <= now; }),
                     toasts.end());
        for (const Toast& t : toasts) {
            lines.push_back({t.text, t.detail, std::chrono::duration<float>(now - t.start).count(),
                             std::chrono::duration<float>(t.until - now).count(), t.tex});
        }
        toast_active = !toasts.empty() || !toast_retired.empty();
    }
    if (lines.empty()) {
        return;
    }
    const ImGuiViewport* viewport = ImGui::GetMainViewport();
    const float s = base_scale;
    const float pad = 20.0f * s, w = 380.0f * s, gap = 10.0f * s;
    const float text_x = 96.0f * s, wrap_w = w - text_x - 16.0f * s;
    const float fs = ImGui::GetFontSize();
    constexpr float kNameScale = 1.3f;
    // Start below the FPS counter when it is showing in the top-right half of the screen.
    float y = viewport->WorkPos.y + pad;
    if (ImGuiWindow* fps = ImGui::FindWindowByName("##fps"); fps && fps->Active) {
        const float mid_x = viewport->WorkPos.x + viewport->WorkSize.x * 0.5f;
        const float mid_y = viewport->WorkPos.y + viewport->WorkSize.y * 0.5f;
        if (fps->Pos.y < mid_y && fps->Pos.x + fps->Size.x > mid_x) {
            y = std::max(y, fps->Pos.y + fps->Size.y + gap);
        }
    }
    for (size_t i = 0; i < lines.size(); i++) {
        const Line& l = lines[i];
        // The card grows with the wrapped name and description (never under 88 px).
        const float name_h = ImGui::CalcTextSize(l.text.c_str(), nullptr, false, wrap_w / kNameScale).y * kNameScale;
        const float detail_h = l.detail.empty() ? 0.0f : ImGui::CalcTextSize(l.detail.c_str(), nullptr, false, wrap_w).y;
        const float block = fs * 1.15f + name_h + (detail_h > 0.0f ? 4.0f * s + detail_h : 0.0f);
        const float h = std::max(88.0f * s, block + 28.0f * s);
        const float tin = std::min(1.0f, l.elapsed / 0.4f);
        const float ease = 1.0f - (1.0f - tin) * (1.0f - tin) * (1.0f - tin); // slides in from the right
        const float alpha = std::clamp(std::min(l.left / 0.5f, l.elapsed / 0.2f), 0.0f, 1.0f);
        const float x = viewport->WorkPos.x + viewport->WorkSize.x - pad + (1.0f - ease) * (w + pad);
        ImGui::SetNextWindowPos(ImVec2(x, y), ImGuiCond_Always, ImVec2(1.0f, 0.0f));
        ImGui::SetNextWindowSize(ImVec2(w, h), ImGuiCond_Always);
        ImGui::PushStyleVar(ImGuiStyleVar_WindowRounding, 10.0f * s);
        ImGui::PushStyleVar(ImGuiStyleVar_WindowBorderSize, 1.5f * s);
        ImGui::PushStyleVar(ImGuiStyleVar_WindowPadding, ImVec2(0.0f, 0.0f));
        ImGui::PushStyleColor(ImGuiCol_WindowBg, ImVec4(0.04f, 0.05f, 0.08f, 0.92f * alpha));
        ImGui::PushStyleColor(ImGuiCol_Border, ImVec4(0.96f, 0.74f, 0.20f, 0.9f * alpha));
        char name[40];
        std::snprintf(name, sizeof(name), "##trophy_toast_%zu", i);
        ImGui::Begin(name, nullptr,
                     ImGuiWindowFlags_NoDecoration | ImGuiWindowFlags_NoInputs | ImGuiWindowFlags_NoNav |
                         ImGuiWindowFlags_NoFocusOnAppearing | ImGuiWindowFlags_NoSavedSettings);
        ImDrawList* dl = ImGui::GetWindowDrawList();
        const ImVec2 p = ImGui::GetWindowPos();
        const ImU32 gold = ImGui::ColorConvertFloat4ToU32(ImVec4(0.96f, 0.74f, 0.20f, alpha));
        const ImU32 goldSoft = ImGui::ColorConvertFloat4ToU32(ImVec4(0.96f, 0.74f, 0.20f, 0.75f * alpha));
        dl->AddRectFilled(p, ImVec2(p.x + 6.0f * s, p.y + h), gold, 10.0f * s, ImDrawFlags_RoundCornersLeft);
        if (l.tex) {
            const float icon = 64.0f * s;
            ImGui::SetCursorPos(ImVec2(18.0f * s, (h - 8.0f * s - icon) * 0.5f));
            ImGui::PushStyleVar(ImGuiStyleVar_Alpha, alpha);
            ImGui::Image(l.tex->GetTexRef(), ImVec2(icon, icon));
            ImGui::PopStyleVar();
            dl->AddRect(ImGui::GetItemRectMin(), ImGui::GetItemRectMax(), gold, 3.0f * s, 0, 2.0f * s);
        } else {
            DrawTrophyIcon(dl, ImVec2(p.x + 52.0f * s, p.y + (h - 8.0f * s) * 0.5f), 28.0f * s, alpha);
        }
        const float frac = std::clamp(l.left / kToastSeconds, 0.0f, 1.0f); // time left
        dl->AddRectFilled(ImVec2(p.x + 14.0f * s, p.y + h - 8.0f * s),
                          ImVec2(p.x + 14.0f * s + (w - 28.0f * s) * frac, p.y + h - 5.0f * s), goldSoft, 1.5f * s);
        const float top = std::max(8.0f * s, (h - 8.0f * s - block) * 0.5f);
        ImGui::PushTextWrapPos(w - 16.0f * s);
        ImGui::SetWindowFontScale(1.0f);
        ImGui::SetCursorPos(ImVec2(text_x, top));
        ImGui::TextColored(ImVec4(0.96f, 0.74f, 0.20f, alpha), "%s", trophy_labels.notice.c_str());
        ImGui::SetCursorPos(ImVec2(text_x, top + fs * 1.15f));
        ImGui::SetWindowFontScale(kNameScale);
        ImGui::TextColored(ImVec4(1.0f, 1.0f, 1.0f, alpha), "%s", l.text.c_str());
        ImGui::SetWindowFontScale(1.0f);
        if (detail_h > 0.0f) {
            ImGui::SetCursorPos(ImVec2(text_x, top + fs * 1.15f + name_h + 4.0f * s));
            ImGui::TextColored(ImVec4(0.80f, 0.83f, 0.90f, alpha), "%s", l.detail.c_str());
        }
        ImGui::PopTextWrapPos();
        ImGui::End();
        ImGui::PopStyleColor(2);
        ImGui::PopStyleVar(3);
        y += h + gap;
    }
}

bool TrophyUnlocked(int id) {
    return trophy_flags && id >= 0 && id < 128 && ((trophy_flags[id / 32] >> (id % 32)) & 1u);
}

void SetTrophyView(bool open) {
    trophy_view = open;
    trophy_scroll = 0.0f;
    trophy_filter = 0;
}

// imgui_mutex held, before NewFrame: the icons of the list become textures (once).
void UpdateTrophyTextures() {
    for (TrophyRow& row : trophy_rows) {
        if (row.tex || row.image.rgba.empty()) {
            continue;
        }
        ImTextureData* tex = IM_NEW(ImTextureData)();
        tex->Create(ImTextureFormat_RGBA32, row.image.width, row.image.height);
        std::memcpy(tex->GetPixels(), row.image.rgba.data(), row.image.rgba.size());
        ImGui::RegisterUserTexture(tex);
        row.tex = tex;
        row.image = {};
    }
}

constexpr float kTrophyRow = 80.0f, kTrophyGap = 6.0f; // layout units (x base_scale)

// The list takes the keys and buttons that scroll, filter and close; the rest of the input stays
// with the game (held neutral by CapturesInput).
bool HandleTrophyEvent(const SDL_Event& event) {
    const float step = (kTrophyRow + kTrophyGap) * base_scale;
    switch (event.type) {
    case SDL_EVENT_KEY_DOWN: {
        const SDL_Keycode key = event.key.key;
        if (key == SDLK_UP || key == SDLK_W) {
            trophy_scroll -= step;
        } else if (key == SDLK_DOWN || key == SDLK_S) {
            trophy_scroll += step;
        } else if (key == SDLK_PAGEUP) {
            trophy_scroll -= 4.0f * step;
        } else if (key == SDLK_PAGEDOWN) {
            trophy_scroll += 4.0f * step;
        } else if (key == SDLK_HOME) {
            trophy_scroll = 0.0f;
        } else if (key == SDLK_END) {
            trophy_scroll = 1.0e6f; // clamped by the window
        } else if (key == SDLK_LEFT || key == SDLK_A || key == SDLK_RIGHT || key == SDLK_D) {
            trophy_filter = (trophy_filter + (key == SDLK_LEFT || key == SDLK_A ? 2 : 1)) % 3;
            trophy_scroll = 0.0f;
        } else if (!event.key.repeat && (key == SDLK_F8 || key == SDLK_ESCAPE || key == SDLK_BACKSPACE ||
                                         key == SDLK_RETURN || key == SDLK_KP_ENTER || key == SDLK_INSERT)) {
            SetTrophyView(false);
        }
        return true;
    }
    case SDL_EVENT_KEY_UP:
        return true;
    case SDL_EVENT_MOUSE_WHEEL:
        trophy_scroll -= event.wheel.y * step;
        return true;
    case SDL_EVENT_GAMEPAD_BUTTON_DOWN:
    case SDL_EVENT_GAMEPAD_BUTTON_UP: {
        const bool down = event.type == SDL_EVENT_GAMEPAD_BUTTON_DOWN;
        const u8 button = event.gbutton.button;
        if (button == SDL_GAMEPAD_BUTTON_RIGHT_STICK) {
            r3_down = down;
            if (down && r2_down) {
                SetTrophyView(false);
            }
        } else if (down) {
            if (button == SDL_GAMEPAD_BUTTON_DPAD_UP) {
                trophy_scroll -= step;
            } else if (button == SDL_GAMEPAD_BUTTON_DPAD_DOWN) {
                trophy_scroll += step;
            } else if (button == SDL_GAMEPAD_BUTTON_DPAD_LEFT || button == SDL_GAMEPAD_BUTTON_LEFT_SHOULDER ||
                       button == SDL_GAMEPAD_BUTTON_DPAD_RIGHT || button == SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER) {
                const bool back = button == SDL_GAMEPAD_BUTTON_DPAD_LEFT || button == SDL_GAMEPAD_BUTTON_LEFT_SHOULDER;
                trophy_filter = (trophy_filter + (back ? 2 : 1)) % 3;
                trophy_scroll = 0.0f;
            } else if (button == SDL_GAMEPAD_BUTTON_EAST || button == SDL_GAMEPAD_BUTTON_SOUTH) {
                SetTrophyView(false);
            }
        }
        return true;
    }
    case SDL_EVENT_GAMEPAD_AXIS_MOTION:
        if (event.gaxis.axis == SDL_GAMEPAD_AXIS_RIGHT_TRIGGER) {
            const bool down = event.gaxis.value > 16000;
            if (down && !r2_down && r3_down) {
                r2_down = true;
                SetTrophyView(false);
                return true;
            }
            r2_down = down;
        } else if (event.gaxis.axis == SDL_GAMEPAD_AXIS_LEFT_TRIGGER) {
            l2_down = event.gaxis.value > 16000;
        }
        return false;
    default:
        return false;
    }
}

std::string TrophyCounter(int have, int total) {
    std::string text = trophy_labels.counter;
    for (const int value : {have, total}) {
        const size_t at = text.find("{}");
        if (at == std::string::npos) {
            break;
        }
        text.replace(at, 2, std::to_string(value));
    }
    return text;
}

// The trophy list, centered over the game: icon, name, description and state of every trophy.
void TrophyView() {
    const ImGuiViewport* viewport = ImGui::GetMainViewport();
    const float s = base_scale;
    const ImVec2 size(std::min(780.0f * s, viewport->WorkSize.x - 40.0f * s),
                      std::min(660.0f * s, viewport->WorkSize.y - 60.0f * s));
    ImGui::SetNextWindowPos(ImVec2(viewport->WorkPos.x + viewport->WorkSize.x * 0.5f,
                                   viewport->WorkPos.y + viewport->WorkSize.y * 0.5f),
                            ImGuiCond_Always, ImVec2(0.5f, 0.5f));
    ImGui::SetNextWindowSize(size, ImGuiCond_Always);
    ImGui::PushStyleVar(ImGuiStyleVar_WindowRounding, 12.0f * s);
    ImGui::PushStyleVar(ImGuiStyleVar_WindowBorderSize, 1.5f * s);
    ImGui::PushStyleVar(ImGuiStyleVar_WindowPadding, ImVec2(0.0f, 0.0f));
    ImGui::PushStyleColor(ImGuiCol_WindowBg, ImVec4(0.03f, 0.04f, 0.07f, 0.96f));
    ImGui::PushStyleColor(ImGuiCol_Border, ImVec4(0.96f, 0.74f, 0.20f, 0.9f));
    ImGui::Begin("##trophy_list", nullptr,
                 ImGuiWindowFlags_NoDecoration | ImGuiWindowFlags_NoMove | ImGuiWindowFlags_NoNav |
                     ImGuiWindowFlags_NoSavedSettings | ImGuiWindowFlags_NoScrollWithMouse |
                     ImGuiWindowFlags_NoFocusOnAppearing);
    ImDrawList* dl = ImGui::GetWindowDrawList();
    const ImVec2 p = ImGui::GetWindowPos();
    ImGui::SetWindowFontScale(1.0f);
    const float fs = ImGui::GetFontSize();
    const ImVec4 gold(0.96f, 0.74f, 0.20f, 1.0f), white(1.0f, 1.0f, 1.0f, 1.0f), muted(0.62f, 0.64f, 0.70f, 1.0f);
    auto color = [](const ImVec4& c, float a = 1.0f) {
        return ImGui::ColorConvertFloat4ToU32(ImVec4(c.x, c.y, c.z, c.w * a));
    };

    int have = 0;
    std::vector<const TrophyRow*> shown;
    for (const TrophyRow& row : trophy_rows) {
        const bool got = TrophyUnlocked(row.id);
        have += got ? 1 : 0;
        if (trophy_filter == 0 || (trophy_filter == 1 && got) || (trophy_filter == 2 && !got)) {
            shown.push_back(&row);
        }
    }

    // header: heading, counter and the filter
    const float title_h = fs * 1.5f;
    const float header_h = 16.0f * s + title_h + 10.0f * s + fs + 16.0f * s;
    const float footer_h = fs + 22.0f * s;
    dl->AddRectFilled(p, ImVec2(p.x + size.x, p.y + header_h), color(ImVec4(0.06f, 0.07f, 0.11f, 1.0f)), 12.0f * s,
                      ImDrawFlags_RoundCornersTop);
    ImGui::SetWindowFontScale(1.5f);
    ImGui::SetCursorPos(ImVec2(24.0f * s, 16.0f * s));
    ImGui::TextColored(gold, "%s", trophy_labels.heading.c_str());
    ImGui::SetWindowFontScale(1.0f);
    const std::string counter = TrophyCounter(have, int(trophy_rows.size()));
    ImGui::SetCursorPos(ImVec2(size.x - 24.0f * s - ImGui::CalcTextSize(counter.c_str()).x, 16.0f * s + (title_h - fs) * 0.5f));
    ImGui::TextColored(white, "%s", counter.c_str());
    const std::string* filters[3] = {&trophy_labels.all, &trophy_labels.only_unlocked, &trophy_labels.only_locked};
    float fx = 24.0f * s;
    const float fy = 16.0f * s + title_h + 10.0f * s;
    for (int i = 0; i < 3; i++) {
        const float w = ImGui::CalcTextSize(filters[i]->c_str()).x;
        ImGui::SetCursorPos(ImVec2(fx, fy));
        ImGui::TextColored(i == trophy_filter ? gold : muted, "%s", filters[i]->c_str());
        if (i == trophy_filter) {
            dl->AddRectFilled(ImVec2(p.x + fx, p.y + fy + fs + 3.0f * s), ImVec2(p.x + fx + w, p.y + fy + fs + 5.0f * s),
                              color(gold), 1.0f * s);
        }
        fx += w + 28.0f * s;
    }

    // list, drawn by hand so it scrolls by pixels
    const ImVec2 list_min(p.x, p.y + header_h), list_max(p.x + size.x, p.y + size.y - footer_h);
    const float step = (kTrophyRow + kTrophyGap) * s;
    const float content_h = float(shown.size()) * step + 12.0f * s;
    const float max_scroll = std::max(0.0f, content_h - (list_max.y - list_min.y));
    trophy_scroll = std::clamp(trophy_scroll, 0.0f, max_scroll);
    const float status_w = std::max(ImGui::CalcTextSize(trophy_labels.unlocked.c_str()).x,
                                    ImGui::CalcTextSize(trophy_labels.locked.c_str()).x);
    ImGui::PushClipRect(list_min, list_max, true);
    for (size_t k = 0; k < shown.size(); k++) {
        const TrophyRow& row = *shown[k];
        const float row_h = kTrophyRow * s;
        const float y = list_min.y + 8.0f * s + float(k) * step - trophy_scroll;
        if (y + row_h < list_min.y || y > list_max.y) {
            continue;
        }
        const bool got = TrophyUnlocked(row.id);
        const ImVec2 a(p.x + 16.0f * s, y), b(p.x + size.x - 16.0f * s - (max_scroll > 0.0f ? 8.0f * s : 0.0f), y + row_h);
        dl->AddRectFilled(a, b, color(got ? ImVec4(0.10f, 0.12f, 0.19f, 0.95f) : ImVec4(0.07f, 0.08f, 0.11f, 0.92f)), 8.0f * s);
        dl->AddRect(a, b, color(got ? ImVec4(0.96f, 0.74f, 0.20f, 0.85f) : ImVec4(0.25f, 0.27f, 0.33f, 0.9f)), 8.0f * s, 0,
                    1.5f * s);
        ImGui::PushClipRect(a, b, true);
        const float icon = 56.0f * s;
        const ImVec2 ip(a.x + 12.0f * s, a.y + (row_h - icon) * 0.5f);
        ImGui::SetCursorScreenPos(ip);
        if (row.tex) {
            ImGui::PushStyleVar(ImGuiStyleVar_Alpha, got ? 1.0f : 0.35f);
            ImGui::Image(row.tex->GetTexRef(), ImVec2(icon, icon));
            ImGui::PopStyleVar();
        } else {
            ImGui::Dummy(ImVec2(icon, icon));
        }
        dl->AddRect(ip, ImVec2(ip.x + icon, ip.y + icon), color(got ? gold : ImVec4(0.30f, 0.32f, 0.38f, 1.0f)), 3.0f * s, 0,
                    1.5f * s);
        const float tx = ip.x + icon + 14.0f * s;
        const float status_x = b.x - 16.0f * s - status_w;
        ImGui::SetWindowFontScale(1.15f);
        ImGui::SetCursorScreenPos(ImVec2(tx, a.y + 9.0f * s));
        ImGui::PushTextWrapPos(status_x - 12.0f * s - p.x);
        ImGui::TextColored(got ? white : muted, "%s", row.name.c_str());
        ImGui::PopTextWrapPos();
        ImGui::SetWindowFontScale(1.0f);
        if (!row.detail.empty()) {
            ImGui::SetCursorScreenPos(ImVec2(tx, a.y + 9.0f * s + fs * 1.15f + 4.0f * s));
            ImGui::PushTextWrapPos(status_x - 12.0f * s - p.x);
            ImGui::TextColored(got ? ImVec4(0.80f, 0.83f, 0.90f, 1.0f) : ImVec4(0.50f, 0.52f, 0.58f, 1.0f), "%s",
                               row.detail.c_str());
            ImGui::PopTextWrapPos();
        }
        ImGui::SetCursorScreenPos(ImVec2(status_x, a.y + (row_h - fs) * 0.5f));
        ImGui::TextColored(got ? gold : muted, "%s", (got ? trophy_labels.unlocked : trophy_labels.locked).c_str());
        ImGui::PopClipRect();
    }
    ImGui::PopClipRect();
    if (max_scroll > 0.0f) { // scroll bar
        const float track = list_max.y - list_min.y - 16.0f * s;
        const float thumb = std::max(24.0f * s, track * (list_max.y - list_min.y) / content_h);
        const float top = list_min.y + 8.0f * s + (track - thumb) * (trophy_scroll / max_scroll);
        dl->AddRectFilled(ImVec2(p.x + size.x - 14.0f * s, top), ImVec2(p.x + size.x - 8.0f * s, top + thumb),
                          color(gold, 0.7f), 3.0f * s);
    }
    // footer: the keys
    const float hint_w = ImGui::CalcTextSize(trophy_labels.hint.c_str()).x;
    ImGui::SetCursorPos(ImVec2(std::max(12.0f * s, (size.x - hint_w) * 0.5f), size.y - footer_h + 10.0f * s));
    ImGui::TextColored(muted, "%s", trophy_labels.hint.c_str());
    ImGui::End();
    ImGui::PopStyleColor(2);
    ImGui::PopStyleVar(3);
}

s64 NowMs() {
    return std::chrono::duration_cast<std::chrono::milliseconds>(
               std::chrono::steady_clock::now().time_since_epoch())
        .count();
}

bool NoticeActive() {
    return notice_until.load(std::memory_order_relaxed) > NowMs();
}

void Notice() {
    std::string text;
    {
        std::scoped_lock lock{notice_mutex};
        text = notice_text;
    }
    const ImGuiViewport* viewport = ImGui::GetMainViewport();
    const float pad = 12.0f * base_scale;
    ImGui::SetNextWindowPos(ImVec2(viewport->WorkPos.x + viewport->WorkSize.x - pad,
                                   viewport->WorkPos.y + viewport->WorkSize.y - pad),
                            ImGuiCond_Always, ImVec2(1.0f, 1.0f));
    ImGui::SetNextWindowBgAlpha(0.6f);
    ImGui::Begin("##notice", nullptr,
                 ImGuiWindowFlags_NoDecoration | ImGuiWindowFlags_AlwaysAutoResize |
                     ImGuiWindowFlags_NoInputs | ImGuiWindowFlags_NoNav |
                     ImGuiWindowFlags_NoFocusOnAppearing);
    ImGui::TextUnformatted(text.c_str());
    ImGui::End();
}

} // namespace

void ShowNotice(const std::string& text, int milliseconds) {
    {
        std::scoped_lock lock{notice_mutex};
        notice_text = text;
    }
    notice_until.store(NowMs() + milliseconds, std::memory_order_relaxed);
}

void Init(const Vulkan::Instance& instance, vk::Format format, u32 image_count) {
    std::scoped_lock lock{imgui_mutex};
    if (initialized) {
        return;
    }
    IMGUI_CHECKVERSION();
    ImGui::CreateContext();
    ImGuiIO& io = ImGui::GetIO();
    io.IniFilename = nullptr; // window positions are not kept
    io.ConfigFlags |= ImGuiConfigFlags_NavEnableKeyboard | ImGuiConfigFlags_NavEnableGamepad;
    io.BackendFlags |= ImGuiBackendFlags_HasGamepad;
    io.BackendPlatformName = "gow3";

    ImGui::StyleColorsDark();
    ImGuiStyle& style = ImGui::GetStyle();
    style.WindowRounding = 6.0f;
    style.FrameRounding = 4.0f;
    style.GrabRounding = 4.0f;
    style.Colors[ImGuiCol_WindowBg].w = 0.92f;

    ImFontConfig font_config;
    font_config.FontDataOwnedByAtlas = false;
    io.Fonts->AddFontFromMemoryTTF(const_cast<unsigned char*>(gow3_font_ttf),
                                   int(gow3_font_ttf_end - gow3_font_ttf), 18.0f, &font_config);

    const vk::Instance vk_instance = instance.GetInstance();
    ImGui_ImplVulkan_LoadFunctions(
        instance.ApiVersion(),
        [](const char* name, void* user) {
            return VULKAN_HPP_DEFAULT_DISPATCHER.vkGetInstanceProcAddr(
                *static_cast<const vk::Instance*>(user), name);
        },
        const_cast<vk::Instance*>(&vk_instance));

    const VkFormat color_format = static_cast<VkFormat>(format);
    ImGui_ImplVulkan_InitInfo info{};
    info.ApiVersion = instance.ApiVersion();
    info.Instance = vk_instance;
    info.PhysicalDevice = instance.GetPhysicalDevice();
    info.Device = instance.GetDevice();
    info.QueueFamily = instance.GetGraphicsQueueFamilyIndex();
    info.Queue = instance.GetGraphicsQueue();
    // Font atlas plus the save list pictures (up to 33, and a closed list's until freed).
    info.DescriptorPoolSize = 128;
    info.MinImageCount = std::max(image_count, 2u);
    info.ImageCount = std::max(image_count, 2u);
    info.UseDynamicRendering = true;
    info.PipelineInfoMain.PipelineRenderingCreateInfo = {
        .sType = VK_STRUCTURE_TYPE_PIPELINE_RENDERING_CREATE_INFO_KHR,
        .colorAttachmentCount = 1,
        .pColorAttachmentFormats = &color_format,
    };
    if (!ImGui_ImplVulkan_Init(&info)) {
        std::printf("Overlay: ImGui Vulkan backend init failed\n");
        ImGui::DestroyContext();
        return;
    }
    overlay_instance = &instance;
    initialized = true;
    std::printf("Overlay: menu ready (Insert or R3+L2)\n");
}

void UpdateTextInput(SDL_Window* window) {
    bool want = false;
    {
        std::scoped_lock lock{imgui_mutex};
        want = initialized && menu_open && ImGui::GetIO().WantTextInput;
    }
    if (want != SDL_TextInputActive(window)) {
        if (want) {
            SDL_StartTextInput(window);
        } else {
            SDL_StopTextInput(window);
        }
    }
}

// imgui_mutex held. The backend frees a texture some frames after its last use.
void RetireChoiceTextures() {
    for (ImTextureData* tex : choice_textures) {
        if (tex) {
            tex->WantDestroyNextFrame = true;
            choice_retired.push_back(tex);
        }
    }
    choice_textures.clear();
    choice_pictures.clear();
    choice_retiring = !choice_retired.empty();
}

// imgui_mutex held, before NewFrame: new pictures become textures, destroyed ones are freed.
void UpdateChoiceTextures() {
    if (!choice_pictures.empty()) {
        choice_textures.assign(choice_pictures.size(), nullptr);
        for (size_t i = 0; i < choice_pictures.size(); ++i) {
            const Gow3Png::Image& picture = choice_pictures[i];
            if (picture.rgba.empty()) {
                continue;
            }
            ImTextureData* tex = IM_NEW(ImTextureData)();
            tex->Create(ImTextureFormat_RGBA32, picture.width, picture.height);
            std::memcpy(tex->GetPixels(), picture.rgba.data(), picture.rgba.size());
            ImGui::RegisterUserTexture(tex);
            choice_textures[i] = tex;
        }
        choice_pictures.clear();
    }
    std::erase_if(choice_retired, [](ImTextureData* tex) {
        if (tex->Status != ImTextureStatus_Destroyed) {
            return false;
        }
        ImGui::UnregisterUserTexture(tex);
        IM_DELETE(tex);
        return true;
    });
    choice_retiring = !choice_retired.empty();
}

// The save list takes the keys and buttons that move and confirm; the rest stays with the game
// (held neutral by CapturesInput).
bool HandleChoiceEvent(const SDL_Event& event) {
    int move = 0, finish = 0; // finish: 1 accept, 2 cancel
    if (event.type == SDL_EVENT_KEY_DOWN) {
        const SDL_Keycode key = event.key.key;
        move = key == SDLK_UP || key == SDLK_W ? -1 : key == SDLK_DOWN || key == SDLK_S ? 1 : 0;
        finish = key == SDLK_RETURN || key == SDLK_KP_ENTER || key == SDLK_SPACE ? 1
                 : key == SDLK_ESCAPE || key == SDLK_BACKSPACE                   ? 2
                                                                                 : 0;
    } else if (event.type == SDL_EVENT_GAMEPAD_BUTTON_DOWN) {
        const u8 button = event.gbutton.button;
        move = button == SDL_GAMEPAD_BUTTON_DPAD_UP ? -1 : button == SDL_GAMEPAD_BUTTON_DPAD_DOWN ? 1 : 0;
        finish = button == SDL_GAMEPAD_BUTTON_SOUTH ? 1 : button == SDL_GAMEPAD_BUTTON_EAST ? 2 : 0;
    } else {
        return event.type == SDL_EVENT_KEY_UP || event.type == SDL_EVENT_GAMEPAD_BUTTON_UP;
    }
    const int count = int(choice_items.size());
    if (move && count) {
        choice_index = (choice_index + move + count) % count;
    }
    if (finish) {
        choice_result = finish == 1 && count ? choice_index : -2;
        choice_active = false;
        RetireChoiceTextures();
    }
    return true;
}

bool HandleEvent(const SDL_Event& event) {
    std::scoped_lock lock{imgui_mutex};
    if (!initialized) {
        return false;
    }
    if (choice_active && !menu_open) {
        return HandleChoiceEvent(event);
    }
    if (trophy_view && !menu_open) {
        return HandleTrophyEvent(event);
    }
    ImGuiIO& io = ImGui::GetIO();
    const bool is_open = menu_open;
    switch (event.type) {
    case SDL_EVENT_KEY_DOWN:
    case SDL_EVENT_KEY_UP: {
        const bool down = event.type == SDL_EVENT_KEY_DOWN;
        static const bool capture_key = [] {
            const char* env = std::getenv("GOW3_FRAME_CAPTURE_KEY");
            return env && *env == '1';
        }();
        if (event.key.key == SDLK_F12) {
            if (down && !event.key.repeat) {
                VideoCore::RequestScreenshot(VideoCore::ScreenshotRequest::GameOnly);
            }
            return true;
        }
        if (down && !event.key.repeat && capture_key && event.key.key == SDLK_F11) {
            Vulkan::FrameCapture::Request();
            return true;
        }
        if (down && !event.key.repeat && event.key.key == SDLK_F8 && !is_open && !text_entry_active && trophy_ready) {
            SetTrophyView(true); // the trophy list
            return true;
        }
        if (down && !event.key.repeat &&
            (event.key.key == SDLK_INSERT || (is_open && event.key.key == SDLK_ESCAPE))) {
            SetOpen(event.key.key == SDLK_INSERT ? !is_open : false);
            return true;
        }
        if (!is_open) {
            return false;
        }
        io.AddKeyEvent(ImGuiMod_Ctrl, (event.key.mod & SDL_KMOD_CTRL) != 0);
        io.AddKeyEvent(ImGuiMod_Shift, (event.key.mod & SDL_KMOD_SHIFT) != 0);
        io.AddKeyEvent(ImGuiMod_Alt, (event.key.mod & SDL_KMOD_ALT) != 0);
        if (const ImGuiKey key = KeyFromSdl(event.key.key); key != ImGuiKey_None) {
            io.AddKeyEvent(key, down);
        }
        return true;
    }
    case SDL_EVENT_GAMEPAD_BUTTON_DOWN:
    case SDL_EVENT_GAMEPAD_BUTTON_UP: {
        const bool down = event.type == SDL_EVENT_GAMEPAD_BUTTON_DOWN;
        const u8 button = event.gbutton.button;
        // R3+L2 opens the menu: L3+R3 is a game action in God of War III.
        if (button == SDL_GAMEPAD_BUTTON_RIGHT_STICK) {
            r3_down = down;
            if (down && l2_down) {
                SetOpen(!is_open);
                return true;
            }
            if (down && r2_down && !is_open && trophy_ready) { // R3+R2: the trophy list
                SetTrophyView(true);
                return true;
            }
        }
        if (!is_open) {
            return false;
        }
        if (const ImGuiKey key = KeyFromGamepad(button); key != ImGuiKey_None) {
            io.AddKeyEvent(key, down);
        }
        return true;
    }
    case SDL_EVENT_GAMEPAD_AXIS_MOTION: {
        if (event.gaxis.axis == SDL_GAMEPAD_AXIS_LEFT_TRIGGER) {
            const bool down = event.gaxis.value > 16000;
            if (down && !l2_down && r3_down) {
                l2_down = true;
                SetOpen(!is_open);
                return true;
            }
            l2_down = down;
        } else if (event.gaxis.axis == SDL_GAMEPAD_AXIS_RIGHT_TRIGGER) {
            const bool down = event.gaxis.value > 16000;
            if (down && !r2_down && r3_down && !is_open && trophy_ready) {
                r2_down = true;
                SetTrophyView(true);
                return true;
            }
            r2_down = down;
        }
        return false;
    }
    case SDL_EVENT_TEXT_INPUT: {
        // Typed characters (Ctrl+click on a slider, a text field): key events alone erase but
        // do not type. SDL sends them while text input is on (UpdateTextInput).
        if (!is_open) {
            return false;
        }
        io.AddInputCharactersUTF8(event.text.text);
        return true;
    }
    case SDL_EVENT_MOUSE_MOTION: {
        if (!is_open) {
            return false;
        }
        const float density = PixelDensity(event.motion.windowID);
        io.AddMousePosEvent(event.motion.x * density, event.motion.y * density);
        return true;
    }
    case SDL_EVENT_MOUSE_BUTTON_DOWN:
    case SDL_EVENT_MOUSE_BUTTON_UP: {
        if (!is_open) {
            return false;
        }
        const int button = event.button.button == SDL_BUTTON_LEFT    ? 0
                           : event.button.button == SDL_BUTTON_RIGHT  ? 1
                           : event.button.button == SDL_BUTTON_MIDDLE ? 2
                                                                      : -1;
        if (button >= 0) {
            io.AddMouseButtonEvent(button, event.type == SDL_EVENT_MOUSE_BUTTON_DOWN);
        }
        return true;
    }
    case SDL_EVENT_MOUSE_WHEEL:
        if (!is_open) {
            return false;
        }
        io.AddMouseWheelEvent(event.wheel.x, event.wheel.y);
        return true;
    default:
        return false;
    }
}

// After "Apply and restart", the new options are kept once the game has shown about ten
// seconds of frames after loading (run.py restores the previous ones otherwise).
void ConfirmRestart() {
    static u32 frames = 0;
    auto& s = Gow3Settings::Get();
    if (!s.restart_unconfirmed || loading || ++frames < 600) {
        return;
    }
    s.restart_unconfirmed = false;
    Gow3Settings::Save();
    std::puts("Settings: graphics options confirmed after restart");
}

void SetBackgroundLoading(u32 done, u32 total) {
    background_done = done;
    background_total = total;
}

void SetLoading(bool active, u32 done, u32 total) {
    loading_done = done;
    loading_total = total;
    loading = active;
}

bool Visible() {
    const auto& s = Gow3Settings::Get();
    return initialized && (loading || background_total || menu_open || text_entry_active || choice_active ||
                           choice_retiring || toast_active || trophy_view || s.show_fps || NoticeActive() ||
                           (s.actor_watch_supported && s.enemy_health_bar));
}

bool CapturesInput() {
    return menu_open || text_entry_active || choice_active || trophy_view;
}

void BeginChoice(const std::string& title, const std::vector<std::string>& items,
                 std::vector<Gow3Png::Image> pictures, int focus) {
    std::scoped_lock lock{imgui_mutex};
    RetireChoiceTextures();
    choice_pictures = std::move(pictures);
    choice_pictures.resize(items.size());
    choice_title = title;
    choice_items = items;
    choice_index = focus >= 0 && focus < int(items.size()) ? focus : 0;
    choice_result = -1;
    choice_active = true;
}

int PollChoice() {
    return choice_result;
}

void SetTextEntry(bool active, const std::string& prompt, const std::string& text) {
    std::scoped_lock lock{imgui_mutex};
    text_entry_active = active;
    text_entry_prompt = prompt;
    text_entry_text = text;
}

void Render(vk::CommandBuffer cmdbuf, vk::ImageView view, vk::Extent2D extent) {
    // Present interval for the FPS readout (measured also while nothing is drawn).
    const auto now = std::chrono::steady_clock::now();
    const float ms = std::chrono::duration<float, std::milli>(now - last_present).count();
    last_present = now;
    if (ms > 0.0f && ms < 1000.0f) {
        frame_ms_avg = frame_ms_avg == 0.0f ? ms : frame_ms_avg * 0.95f + ms * 0.05f;
    }
    ConfirmRestart();
    if (!Visible()) {
        return;
    }
    std::scoped_lock lock{imgui_mutex};
    ImGuiIO& io = ImGui::GetIO();
    io.DisplaySize = ImVec2(float(extent.width), float(extent.height));
    io.DeltaTime = ms > 0.0f && ms < 1000.0f ? ms / 1000.0f : 1.0f / 60.0f;
    // UI scale follows the display height (1080p = 1).
    const float scale = std::max(float(extent.height) / 1080.0f, 0.75f);
    if (std::abs(scale - base_scale) > 0.01f) {
        ImGuiStyle& style = ImGui::GetStyle();
        style.ScaleAllSizes(scale / base_scale);
        style.FontScaleMain = scale;
        base_scale = scale;
    }

    UpdateChoiceTextures();
    UpdateToastTextures();
    UpdateTrophyTextures();
    ImGui_ImplVulkan_NewFrame();
    ImGui::NewFrame();
    if (loading) {
        LoadingScreen();
    }
    if (menu_open) {
        Menu();
    }
    if (Gow3Settings::Get().show_fps && !menu_open) {
        FpsCounter();
    }
    if (NoticeActive()) {
        Notice();
    }
    if (!loading && !menu_open) {
        HealthDisplay();
    }
    if (background_total && !loading && !menu_open) {
        BackgroundLoading();
    }
    if (text_entry_active) {
        TextEntryBox();
    }
    if (choice_active && !menu_open) {
        ChoiceBox();
    }
    if (trophy_view && !menu_open) {
        TrophyView();
    }
    if (toast_active) {
        ToastBox();
    }
    ImGui::Render();

    const vk::RenderingAttachmentInfo attachment{
        .imageView = view,
        .imageLayout = vk::ImageLayout::eColorAttachmentOptimal,
        .loadOp = vk::AttachmentLoadOp::eLoad,
        .storeOp = vk::AttachmentStoreOp::eStore,
    };
    cmdbuf.beginRendering(vk::RenderingInfo{
        .renderArea = {{0, 0}, extent},
        .layerCount = 1,
        .colorAttachmentCount = 1,
        .pColorAttachments = &attachment,
    });
    {
        // Font atlas uploads submit to the graphics queue themselves.
        std::scoped_lock submit_lock{Vulkan::Scheduler::submit_mutex};
        ImGui_ImplVulkan_RenderDrawData(ImGui::GetDrawData(), cmdbuf);
    }
    cmdbuf.endRendering();
}

} // namespace Gow3Overlay

// Called by the runtime (src/runtime_services.c) when the game unlocks a trophy. title: its name,
// detail: its description (may be empty), png: the trophy's icon (the PNG bytes from trophy00.trp)
// or null for the drawn medal.
extern "C" void gow3gpu_toast_trophy(const char* title, const char* detail, const unsigned char* png,
                                     uint64_t png_size) {
    if (!title || !title[0]) {
        return;
    }
    Gow3Png::Image image;
    if (png && png_size) {
        Gow3Png::DecodeRgba(png, size_t(png_size), image); // empty on failure: the medal is drawn
    }
    using namespace Gow3Overlay;
    std::scoped_lock lock{toast_mutex};
    const auto now = std::chrono::steady_clock::now();
    if (toasts.size() >= 6) {
        toasts.front().until = now; // expires now; Render retires its texture
    }
    toasts.push_back({title, detail ? detail : "", now,
                      now + std::chrono::milliseconds(int(kToastSeconds * 1000.0f)), std::move(image)});
    toast_active = true;
}

extern "C" void gow3gpu_toast_icon(const char* text, const unsigned char* png, uint64_t png_size) {
    gow3gpu_toast_trophy(text, nullptr, png, png_size);
}

extern "C" void gow3gpu_toast(const char* text) {
    gow3gpu_toast_trophy(text, nullptr, nullptr, 0);
}

// Called once by the runtime (src/runtime_services.c) with the trophy texts: the in-game list
// (F8 / R3+R2) and the notice labels. The icons are decoded on a worker thread; unlocked is read live.
extern "C" void gow3gpu_trophy_setup(const Gow3GpuTrophyLabels* labels, const Gow3GpuTrophyInfo* items, int count,
                                     const volatile uint32_t* unlocked) {
    using namespace Gow3Overlay;
    if (!items || count <= 0 || trophy_started.exchange(true)) {
        return;
    }
    struct Item {
        int id;
        std::string name, detail;
        std::vector<unsigned char> png;
    };
    std::vector<Item> list;
    for (int i = 0; i < count; i++) {
        list.push_back({items[i].id, items[i].name ? items[i].name : "", items[i].detail ? items[i].detail : "",
                        items[i].png && items[i].png_size
                            ? std::vector<unsigned char>(items[i].png, items[i].png + items[i].png_size)
                            : std::vector<unsigned char>()});
    }
    TrophyLabels copy;
    if (labels) {
        auto take = [](std::string& to, const char* from) {
            if (from && from[0]) {
                to = from;
            }
        };
        take(copy.heading, labels->heading);
        take(copy.notice, labels->notice);
        take(copy.unlocked, labels->status_unlocked);
        take(copy.locked, labels->status_locked);
        take(copy.all, labels->filter_all);
        take(copy.only_unlocked, labels->filter_unlocked);
        take(copy.only_locked, labels->filter_locked);
        take(copy.counter, labels->counter);
        take(copy.hint, labels->hint);
    }
    std::thread([list = std::move(list), copy = std::move(copy), unlocked]() mutable {
        std::vector<TrophyRow> rows;
        for (Item& item : list) {
            TrophyRow row;
            row.id = item.id;
            row.name = std::move(item.name);
            row.detail = std::move(item.detail);
            if (!item.png.empty()) {
                Gow3Png::DecodeRgba(item.png.data(), item.png.size(), row.image); // empty on failure
            }
            rows.push_back(std::move(row));
        }
        std::scoped_lock lock{imgui_mutex};
        trophy_rows = std::move(rows);
        trophy_labels = std::move(copy);
        trophy_flags = unlocked;
        trophy_ready = true;
    }).detach();
}
