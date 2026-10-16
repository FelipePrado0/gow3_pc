// SPDX-License-Identifier: GPL-2.0-or-later
// Saved FSR 4 choices must be safe before the first rendered frame on older GPUs, and saved
// MetalFX choices where MetalFX cannot run.
#include <cassert>
#include <cstdio>
#include <cstdlib>
#include <initializer_list>
#include <string>
#include <unistd.h>
#include "gow3_settings.h"

int main() {
    using namespace Gow3Settings;
    char path[] = "/tmp/gow3-upscaler-test-XXXXXX";
    const int fd = mkstemp(path);
    assert(fd >= 0);
    close(fd);
    setenv("GOW3_CONFIG", path, 1);
    unsetenv("GOW3_UPSCALER");
    unsetenv("GOW3_UPSCALE_PRESET");
    unsetenv("GOW3_RENDER_RES");
    auto& s = Get();
    s.upscaler = UpscalerTaa;
    s.preset = Performance;
    assert(RenderPreset() == NativeAA && s.preset == Performance);
    Save();
    s.upscaler = UpscalerOff;
    Load();
    assert(s.upscaler == UpscalerTaa && s.preset == Performance);
    assert(RenderPreset() == NativeAA && !ResolutionNeedsRestart());
    s.upscaler = UpscalerFsr3;
    assert(RenderPreset() == Performance);
    for (bool fsr4 : {false, true}) {
        for (bool fsr411 : {false, true}) {
            for (int requested = 0; requested < UpscalerCount; ++requested) {
                FILE* config = std::fopen(path, "w");
                assert(config);
                std::fprintf(config, "upscaler=%s\npreset=3\noutput_res=2560x1440\n",
                             UpscalerName(requested));
                std::fclose(config);
                s.fsr4_problem = nullptr;
                Load();
                assert(s.upscaler == requested);
                ConfigureUpscalerSupport(fsr4, fsr411);
                const bool unsupported = (requested == UpscalerFsr4 && !fsr4) ||
                    (requested == UpscalerFsr411 && !(fsr4 && fsr411));
                assert(s.upscaler == (unsupported ? UpscalerFsr3 : requested));
                assert(s.fsr4_supported == fsr4);
                assert(s.fsr411_supported == (fsr4 && fsr411));
                assert(bool(s.fsr4_problem.load()) == unsupported);
                assert(s.preset == Performance && s.output_res == 2);
                // Startup patch settings still describe the already applied guest patches.
                assert(s.startup_upscaler == requested);
                ConfigureUpscalerSupport(fsr4, fsr411);
                assert(s.upscaler == (unsupported ? UpscalerFsr3 : requested));
            }
        }
    }
    // MetalFX (macOS): a saved choice runs FSR 3.1 when MetalFX or its Vulkan interop is missing.
    for (bool available : {false, true}) {
        s.upscaler = UpscalerMetalFx;
        ConfigureMetalFxSupport(available, available ? "" : "no VK_EXT_external_memory_metal");
        assert(s.upscaler == (available ? UpscalerMetalFx : UpscalerFsr3));
        assert(s.metalfx_supported == available);
        assert(bool(s.metalfx_problem.load()) == !available);
    }
    s.upscaler = UpscalerFsr4;
    ConfigureMetalFxSupport(false, "MetalFX needs macOS");
    assert(s.upscaler == UpscalerFsr4); // other choices stay
    s.upscaler = UpscalerMetalFx;
    Save();
    s.upscaler = UpscalerOff;
    Load();
    assert(s.upscaler == UpscalerMetalFx && std::string{UpscalerName(s.upscaler)} == "metalfx");
    setenv("GOW3_UPSCALER", "metalfx", 1);
    s.upscaler = UpscalerFsr3;
    Load();
    assert(s.upscaler == UpscalerMetalFx);
    unsetenv("GOW3_UPSCALER");
    s.startup_preset = Quality;
    s.startup_upscaler = UpscalerFsr3;
    s.upscaler = UpscalerFsr3;
    s.preset = Performance;
    assert(RenderPreset() == Performance && !ResolutionNeedsRestart());
    setenv("GOW3_RENDER_RES", "1706x960", 1);
    assert(RenderPreset() == Quality && ResolutionNeedsRestart());
    s.preset = Quality;
    assert(!ResolutionNeedsRestart());
    s.upscaler = UpscalerOff;
    assert(ResolutionNeedsRestart());
    s.upscaler = UpscalerFsr4;
    assert(!ResolutionNeedsRestart()); // provider change at the same patched size is live
    s.upscaler = UpscalerTaa;          // TAA needs native guest targets
    assert(ResolutionNeedsRestart());
    s.upscaler = UpscalerFsr4;
    const int output = s.startup_output_res;
    s.output_res = (output + 1) % OutputCount;
    assert(ResolutionNeedsRestart());
    s.output_res = output;
    setenv("GOW3_RENDER_RES", "", 1);
    assert(!FixedRenderSession() && !ResolutionNeedsRestart());
    unsetenv("GOW3_RENDER_RES");
    std::remove(path);
    std::puts("Upscaler support: saved choices, FSR 3.1 and MetalFX fallbacks PASS");
}
