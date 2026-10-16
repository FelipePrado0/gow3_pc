// SPDX-License-Identifier: GPL-2.0-or-later
// Cutscene bars (video_core/renderer_vulkan/movie_bars.h): the game draws its H.264 cutscenes in
// a centered 16:9 picture and never clears the rest of the display buffer, so at the Ultrawide
// mod's sizes the last 3D frame stayed at the sides (or at the top and bottom on 16:10). While
// the video decoder hands out pictures, everything outside that picture is drawn black.
// No game or Vulkan device required.
#include <cassert>
#include <cmath>
#include <cstdio>
#include <cstdlib>

#include "video_core/renderer_vulkan/movie_bars.h"

using namespace Vulkan;

static bool Near(float a, float b) {
    return std::fabs(a - b) < 1e-5f;
}

static bool Is(MovieBars::Rect r, float x0, float y0, float x1, float y1) {
    return Near(r.x0, x0) && Near(r.y0, y0) && Near(r.x1, x1) && Near(r.y1, y1);
}

int main() {
#ifdef _WIN32
    _set_error_mode(_OUT_TO_STDERR);
#endif
    // Wider than 16:9: the picture is 16:9 at full height, centered (5120x1440: 2560 wide).
    assert(Is(MovieBars::Picture(5120, 1440), 0.25f, 0.0f, 0.75f, 1.0f));
    // 3440x1440: 2560 wide, 440 pixels of bar on each side.
    assert(Is(MovieBars::Picture(3440, 1440), 440.0f / 3440, 0.0f, 3000.0f / 3440, 1.0f));
    // Taller than 16:9 (16:10): full width, 1440 of 1600 rows, 80 rows of bar above and below.
    assert(Is(MovieBars::Picture(2560, 1600), 0.0f, 0.05f, 1.0f, 0.95f));
    // 16:9 and the built-in sizes (480p is 854x480, 0.07% off 16:9) have no bars.
    assert(Is(MovieBars::Picture(1920, 1080), 0.0f, 0.0f, 1.0f, 1.0f));
    assert(Is(MovieBars::Picture(854, 480), 0.0f, 0.0f, 1.0f, 1.0f));
    assert(Is(MovieBars::Picture(3840, 2160), 0.0f, 0.0f, 1.0f, 1.0f));
    assert(Is(MovieBars::Picture(0, 0), 0.0f, 0.0f, 1.0f, 1.0f));

    // Bars only while a cutscene plays: the game decodes 30 pictures a second, so the last one
    // is at most a few frames old; once decoding stops the whole frame shows again.
    MovieBars::Clock clock;
    assert(Is(clock.Bars(5120, 1440, 10'000), 0.0f, 0.0f, 1.0f, 1.0f)); // no picture yet
    clock.NotePicture(10'000);
    assert(Is(clock.Bars(5120, 1440, 10'000), 0.25f, 0.0f, 0.75f, 1.0f));
    assert(Is(clock.Bars(5120, 1440, 10'100), 0.25f, 0.0f, 0.75f, 1.0f));
    assert(Is(clock.Bars(5120, 1440, 11'000), 0.0f, 0.0f, 1.0f, 1.0f)); // video over
    clock.NotePicture(12'000);
    assert(Is(clock.Bars(2560, 1600, 12'050), 0.0f, 0.05f, 1.0f, 0.95f));
    assert(Is(clock.Bars(1920, 1080, 12'050), 0.0f, 0.0f, 1.0f, 1.0f));

    std::puts("movie bars OK");
    return EXIT_SUCCESS;
}
