// SPDX-License-Identifier: GPL-2.0-or-later
// gow3: graphics options of the in-game menu (Insert / R3+L2), kept in gow3.ini. The live ones
// apply on the next frame; the startup ones (patches, GPU readback modes) are read by run.py and
// apply through "Apply and restart". The launcher reads and writes the same keys.
#pragma once

#include <algorithm>
#include <array>
#include <cstdlib>
#include <string>
#include <string_view>
#include <vector>

namespace Gow3Graphics {

enum DisplayMode : int { Windowed = 0, Borderless = 1, Fullscreen = 2, DisplayModeCount };
inline constexpr std::array<std::string_view, DisplayModeCount> DisplayModeKeys = {
    "windowed", "borderless", "fullscreen"};
inline constexpr std::array<const char*, DisplayModeCount> DisplayModeNames = {
    "Windowed", "Borderless", "Fullscreen"};

/// Present rate limits; 0 = unlimited.
inline constexpr std::array<int, 5> FpsLimits = {30, 60, 120, 240, 0};

/// Game patch suffixes ("Resolution Patch - <name>"); "native" is no resolution patch.
/// The last six are the Ultrawide mod's sizes (#202), built in as beta (scripts/graphics_options.py).
inline constexpr std::array<std::string_view, 12> Resolutions = {
    "native", "480p", "720p", "1440p", "1800p", "4K",
    "2560x1080", "3440x1440", "3840x1600", "5120x1440", "1920x1200", "2560x1600"};
/// The resolutions to choose from: the built-in ones, then those of the enabled mods' patch files
/// (GOW3_EXTRA_RESOLUTIONS, ";"-separated sizes such as "3440x1440"; run.py sets it at every start).
inline std::vector<std::string> ResolutionNames() {
    std::vector<std::string> names(Resolutions.begin(), Resolutions.end());
    const char* extra = std::getenv("GOW3_EXTRA_RESOLUTIONS");
    for (std::string_view rest = extra ? extra : ""; !rest.empty();) {
        const std::size_t end = rest.find(';');
        const std::string_view name = rest.substr(0, end);
        if (!name.empty() && std::find(names.begin(), names.end(), name) == names.end()) names.emplace_back(name);
        rest = end == std::string_view::npos ? std::string_view() : rest.substr(end + 1);
    }
    return names;
}
/// Engine frame rate ceilings: 60 is the game's own, 120 and 240 are frame rate patches.
inline constexpr std::array<int, 3> EngineFps = {60, 120, 240};

template <typename T, std::size_t N>
constexpr int IndexOf(const std::array<T, N>& values, const T& value, int fallback) {
    for (std::size_t i = 0; i < N; ++i) {
        if (values[i] == value) return int(i);
    }
    return fallback;
}

inline int ParseDisplayMode(std::string_view text) {
    return IndexOf(DisplayModeKeys, text, Windowed);
}
/// A listed limit, else unlimited.
inline int ParseFpsLimit(int value) {
    return IndexOf(FpsLimits, value, -1) >= 0 ? value : 0;
}
/// Index into ResolutionNames(); unknown text is native.
inline int ParseResolution(std::string_view text) {
    const std::vector<std::string> names = ResolutionNames();
    const auto found = std::find(names.begin(), names.end(), text);
    return found == names.end() ? 0 : int(found - names.begin());
}
/// A listed ceiling, else 120 (the default patch).
inline int ParseEngineFps(int value) {
    return IndexOf(EngineFps, value, -1) >= 0 ? value : 120;
}

/// Anisotropic filtering forced on the game's mipmapped textures; 0 = the game's own. Samplers
/// are cached, so it applies on restart.
inline constexpr std::array<int, 5> Anisotropies = {0, 2, 4, 8, 16};
inline int ParseAnisotropy(int value) {
    return IndexOf(Anisotropies, value, -1) >= 0 ? value : 0;
}

/// Frames the GPU command thread may queue ahead of the GPU; 0 = unbounded.
inline constexpr std::array<int, 3> FramesQueued = {1, 2, 0};
inline int ParseFramesQueued(int value) {
    return IndexOf(FramesQueued, value, -1) >= 0 ? value : 1;
}
/// RCAS strength in percent; invalid values fall back to 75.
inline int ParseRcasStrength(int value) {
    return value >= 0 && value <= 100 ? value : 75;
}
/// FSR RCAS attenuation for a strength: 0 is the sharpest, 2 the softest.
inline float RcasAttenuation(int strength) {
    return 2.0f * (1.0f - float(ParseRcasStrength(strength)) / 100.0f);
}
/// Whether the FSR pass runs: FSR 1 when the window is larger than the game's image, RCAS
/// alone when they are the same size. A smaller window is left to the regular scaling.
inline bool FsrPassRuns(bool fsr1, bool rcas, int in_w, int in_h, int out_w, int out_h) {
    const bool upscale = in_w < out_w && in_h < out_h;
    const bool same = in_w == out_w && in_h == out_h;
    return (fsr1 && upscale) || (rcas && same);
}

/// What the present limit really allows: the engine ceiling caps it.
inline int EffectiveFps(int limit, int engine_fps) {
    return limit == 0 || limit > engine_fps ? engine_fps : limit;
}

/// The present thread's frame limit: never above the engine rate the patches applied, since the
/// engine steps a fixed time per frame and faster flips speed the game up. 0 stays vblank paced.
inline int PresentLimit(int limit, int engine_fps) {
    return limit == 0 ? 0 : EffectiveFps(limit, engine_fps);
}

} // namespace Gow3Graphics
