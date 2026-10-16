// SPDX-License-Identifier: GPL-2.0-or-later
// gow3: numbers for the performance overlay (gow3_overlay.cpp). A thread started once samples
// CPU, RAM, GPU use (Windows counters) and VRAM (VK_EXT_memory_budget) twice a second;
// a value that cannot be measured stays negative and shows as "n/a".

#pragma once

#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <string>

namespace Vulkan {
class Instance;
}

namespace Gow3PerfStats {

/// Overlay items (Gow3Settings::Values::overlay_items).
enum Item : unsigned {
    Fps = 1u << 0,
    FrameTime = 1u << 1,
    Upscaler = 1u << 2,
    Gpu = 1u << 3,
    Cpu = 1u << 4,
    Ram = 1u << 5,
    Vram = 1u << 6,
    AllItems = (1u << 7) - 1,
};
inline constexpr unsigned DefaultItems = Fps | FrameTime | Upscaler;

struct Sample {
    float fps = 0.0f, frame_ms = 0.0f;
    std::string upscaler;
    std::string gpu_name;
    float gpu_percent = -1.0f, cpu_percent = -1.0f;
    double ram_used_gb = -1.0, ram_total_gb = -1.0, vram_used_gb = -1.0, vram_total_gb = -1.0;
};

/// Process CPU time over wall time on every core, 0 to 100; -1 without a wall interval.
inline float CpuPercent(uint64_t process_delta, uint64_t wall_delta, unsigned cores) {
    if (!wall_delta || !cores) {
        return -1.0f;
    }
    return std::clamp(float(double(process_delta) * 100.0 / (double(wall_delta) * cores)), 0.0f, 100.0f);
}

/// The overlay's upscaler item: the temporal upscaler's name when one is chosen, else what brings
/// the game's image to the window: "MetalFX 720p->1080p" or "FSR 1 720p->1080p" (FSR 1 also
/// where MetalFX cannot run), "Stretched ..." with upscaling off, "Downscaled ..." for an image
/// larger than the window, else "Native 1080p". Sizes in pixels, 0 before the first frame.
inline std::string UpscalerText(const char* temporal, bool fsr1, bool metalfx, int image_w, int image_h,
                                int window_w, int window_h) {
    if (temporal && temporal[0]) {
        return temporal;
    }
    if (image_h <= 0 || window_h <= 0) {
        return "Native";
    }
    const bool upscale = image_w < window_w && image_h < window_h;
    const bool downscale = image_w > window_w || image_h > window_h;
    const char* how = upscale ? (metalfx ? "MetalFX" : fsr1 ? "FSR 1" : "Stretched") : downscale ? "Downscaled" : nullptr;
    char text[64];
    if (how) {
        std::snprintf(text, sizeof(text), "%s %dp->%dp", how, image_h, window_h);
    } else {
        std::snprintf(text, sizeof(text), "Native %dp", image_h);
    }
    return text;
}

/// The overlay text for the items in `mask`: one line each, or one line when `row`.
inline std::string Format(const Sample& s, unsigned mask, bool row) {
    std::string out;
    char part[160];
    const auto add = [&] {
        if (!out.empty()) {
            out += row ? "   " : "\n";
        }
        out += part;
    };
    const auto pair = [](char* to, size_t size, const char* label, double used, double total) {
        if (used < 0.0) {
            std::snprintf(to, size, "%s n/a", label);
        } else if (total < 0.0) {
            std::snprintf(to, size, "%s %.1f GB", label, used);
        } else {
            std::snprintf(to, size, "%s %.1f / %.1f GB", label, used, total);
        }
    };
    if (mask & Fps) {
        std::snprintf(part, sizeof(part), "%.0f FPS", s.fps);
        add();
    }
    if (mask & FrameTime) {
        std::snprintf(part, sizeof(part), "%.1f ms", s.frame_ms);
        add();
    }
    if (mask & Upscaler) {
        std::snprintf(part, sizeof(part), "%s", s.upscaler.empty() ? "Native" : s.upscaler.c_str());
        add();
    }
    if (mask & Gpu) {
        const char* name = s.gpu_name.empty() ? "GPU" : s.gpu_name.c_str();
        if (s.gpu_percent < 0.0f) {
            std::snprintf(part, sizeof(part), "%s n/a", name);
        } else {
            std::snprintf(part, sizeof(part), "%s %.0f%%", name, s.gpu_percent);
        }
        add();
    }
    if (mask & Cpu) {
        if (s.cpu_percent < 0.0f) {
            std::snprintf(part, sizeof(part), "CPU n/a");
        } else {
            std::snprintf(part, sizeof(part), "CPU %.0f%%", s.cpu_percent);
        }
        add();
    }
    if (mask & Ram) {
        pair(part, sizeof(part), "RAM", s.ram_used_gb, s.ram_total_gb);
        add();
    }
    if (mask & Vram) {
        pair(part, sizeof(part), "VRAM", s.vram_used_gb, s.vram_total_gb);
        add();
    }
    return out;
}

/// Starts the sampling thread the first time; later calls do nothing.
void Start(const Vulkan::Instance& instance);
/// The latest CPU, RAM, GPU and VRAM values (the caller fills FPS, frametime and upscaler).
Sample Latest();

} // namespace Gow3PerfStats
