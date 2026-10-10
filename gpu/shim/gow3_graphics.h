// SPDX-License-Identifier: GPL-2.0-or-later
// gow3: graphics options of the in-game menu (Insert / R3+L2), kept in gow3.ini. The live ones
// apply on the next frame; the startup ones (patches, GPU readback modes) are read by run.py and
// apply through "Apply and restart". The launcher reads and writes the same keys.
#pragma once

#include <array>
#include <string_view>

namespace Gow3Graphics {

enum DisplayMode : int { Windowed = 0, Borderless = 1, Fullscreen = 2, DisplayModeCount };
inline constexpr std::array<std::string_view, DisplayModeCount> DisplayModeKeys = {
    "windowed", "borderless", "fullscreen"};
inline constexpr std::array<const char*, DisplayModeCount> DisplayModeNames = {
    "Windowed", "Borderless", "Fullscreen"};

/// Present rate limits; 0 = unlimited.
inline constexpr std::array<int, 5> FpsLimits = {30, 60, 120, 240, 0};

/// Game patch suffixes ("Resolution Patch - <name>"); "native" is no resolution patch.
inline constexpr std::array<std::string_view, 6> Resolutions = {"native", "480p", "720p",
                                                                "1440p", "1800p", "4K"};
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
/// Index into Resolutions; unknown text is native.
inline int ParseResolution(std::string_view text) {
    return IndexOf(Resolutions, text, 0);
}
/// A listed ceiling, else 120 (the default patch).
inline int ParseEngineFps(int value) {
    return IndexOf(EngineFps, value, -1) >= 0 ? value : 120;
}

/// What the present limit really allows: the engine ceiling caps it.
inline int EffectiveFps(int limit, int engine_fps) {
    return limit == 0 || limit > engine_fps ? engine_fps : limit;
}

} // namespace Gow3Graphics
