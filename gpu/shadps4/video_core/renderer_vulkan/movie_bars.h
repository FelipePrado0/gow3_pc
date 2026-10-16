// SPDX-License-Identifier: GPL-2.0-or-later
#pragma once

#include <atomic>
#include <chrono>
#include <cmath>
#include <cstdint>

// gow3: the game draws its H.264 cutscenes (libSceVideodec) in a centered 16:9 picture, as it
// fits its 2D layer to the display aspect ratio, and never clears the rest of the display buffer:
// at 16:9 the picture covers all of it. At the Ultrawide mod's sizes the last 3D frame stayed
// around the picture, so while the decoder hands out pictures the presenter draws it black.
namespace Vulkan::MovieBars {

// Normalized coordinates of the display buffer.
struct Rect {
    float x0, y0, x1, y1;
};

constexpr Rect Whole{0.0f, 0.0f, 1.0f, 1.0f};

// The 16:9 picture centered in a width x height display buffer; the whole buffer when it is
// 16:9 within 1% (the built-in sizes: 480p is 854x480).
inline Rect Picture(uint32_t width, uint32_t height) {
    if (!width || !height) {
        return Whole;
    }
    const float aspect = float(width) / float(height), wide = 16.0f / 9.0f;
    if (std::abs(aspect / wide - 1.0f) < 0.01f) {
        return Whole;
    }
    if (aspect > wide) {
        const float side = (1.0f - wide / aspect) / 2.0f;
        return {side, 0.0f, 1.0f - side, 1.0f};
    }
    const float top = (1.0f - aspect / wide) / 2.0f;
    return {0.0f, top, 1.0f, 1.0f - top};
}

// When the decoder last handed out a picture, and the part of the display buffer to show.
class Clock {
public:
    // Pictures come 30 a second; the cutscene is over once none came for this long.
    static constexpr int64_t HoldMs = 250;

    void NotePicture(int64_t now_ms) {
        last_ms.store(now_ms, std::memory_order_relaxed);
    }

    Rect Bars(uint32_t width, uint32_t height, int64_t now_ms) const {
        const int64_t at = last_ms.load(std::memory_order_relaxed);
        return at != Never && now_ms - at <= HoldMs ? Picture(width, height) : Whole;
    }

private:
    static constexpr int64_t Never = INT64_MIN;
    std::atomic<int64_t> last_ms{Never};
};

inline int64_t NowMs() {
    using namespace std::chrono;
    return duration_cast<milliseconds>(steady_clock::now().time_since_epoch()).count();
}

// The game's cutscenes: the video decoder notes its pictures, the presenter reads the bars.
inline Clock& Cutscenes() {
    static Clock clock;
    return clock;
}

} // namespace Vulkan::MovieBars
