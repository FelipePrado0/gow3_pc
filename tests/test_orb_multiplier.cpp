#include <cassert>
#include <cmath>
#include <limits>
#include "gow3_orbs.h"
#include "../src/orb_hook.h"
#include <vector>
#include <array>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include "gow3_settings.h"
#ifdef _WIN32
#include <windows.h>
#else
#include <sys/mman.h>
#endif

extern "C" float __attribute__((sysv_abi)) gow3_orbs_gain(void*, float);
extern "C" void gow3_orbs_status(int);
static void __attribute__((sysv_abi)) Setter(void* player, float balance) {
    balance = std::min(balance, 999999.0f);
    std::memcpy(static_cast<unsigned char*>(player) + 0x1e8, &balance, 4);
}

int main(int argc, char** argv) {
    assert(Gow3Orbs::Scale(100, 110, 2) == 120);
    assert(Gow3Orbs::Scale(100, 90, 100) == 90);
    assert(Gow3Orbs::Scale(100, 100, 100) == 100);
    assert(Gow3Orbs::Scale(100, 110, 1) == 110);
    assert(Gow3Orbs::Scale(0, 1, 100) == 100);
    float balance = 0;
    for (int i = 0; i < 10; ++i) balance = Gow3Orbs::Scale(balance, balance + 1, 0.1f);
    assert(std::abs(balance - 1) < 0.00001f);
    assert(Gow3Orbs::Scale(balance, balance + 2, 2) == balance + 4);
    assert(Gow3Orbs::Scale(100, 110, std::numeric_limits<float>::quiet_NaN()) == 110);
    assert(Gow3Orbs::ClampMultiplier(0) == 0.1f);
    assert(Gow3Orbs::ClampMultiplier(101) == 100);
    assert(Gow3Orbs::ClampMultiplier(std::numeric_limits<float>::infinity()) == 1);
    std::vector<unsigned char> image(0x6fbdc);
    assert(!gow3_orb_signatures_match(image.data(), image.size()));
    assert(!gow3_orb_signatures_match(image.data(), 1));
    if (argc > 1) {
        FILE* file = std::fopen(argv[1], "rb");
        assert(file && !std::fseek(file, 0x4000, SEEK_SET));
        assert(std::fread(image.data(), 1, image.size(), file) == image.size());
        std::fclose(file);
        assert(gow3_orb_signatures_match(image.data(), image.size()));
        image[gow3_orb_calls[1]] ^= 1;
        assert(!gow3_orb_signatures_match(image.data(), image.size()));
    }
#ifdef _WIN32
    auto* stub = static_cast<unsigned char*>(VirtualAlloc(nullptr, 4096, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE));
#else
    auto* stub = static_cast<unsigned char*>(mmap(nullptr, 4096, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0));
    assert(stub != MAP_FAILED);
#endif
    assert(stub);
    gow3_orb_build_stub(stub, reinterpret_cast<uintptr_t>(gow3_orbs_gain), reinterpret_cast<uintptr_t>(Setter));
#ifdef _WIN32
    DWORD previous;
    assert(VirtualProtect(stub, 4096, PAGE_EXECUTE_READ, &previous));
    FlushInstructionCache(GetCurrentProcess(), stub, 26);
#else
    assert(!mprotect(stub, 4096, PROT_READ | PROT_EXEC));
#endif
    using Hook = void (__attribute__((sysv_abi)) *)(void*, float);
    auto hook = reinterpret_cast<Hook>(stub);
    std::array<unsigned char, 0x200> player{};
    balance = 100;
    std::memcpy(player.data() + 0x1e8, &balance, 4);
    auto& settings = Gow3Settings::Get();
    settings.red_orb_multiplier = 2;
    hook(player.data(), 110);
    std::memcpy(&balance, player.data() + 0x1e8, 4);
    assert(balance == 120);
    settings.red_orb_multiplier = 100;
    hook(player.data(), 90);
    std::memcpy(&balance, player.data() + 0x1e8, 4);
    assert(balance == 90);
    hook(player.data(), 100000);
    std::memcpy(&balance, player.data() + 0x1e8, 4);
    assert(balance == 999999);
    gow3_orbs_status(1);
    assert(settings.red_orbs_supported);
    gow3_orbs_status(0);
    assert(!settings.red_orbs_supported);
    auto config = std::filesystem::temp_directory_path() / "gow3-orb-multiplier-test.ini";
    assert(!std::filesystem::exists(config));
#ifdef _WIN32
    _putenv_s("GOW3_CONFIG", config.string().c_str());
#else
    setenv("GOW3_CONFIG", config.string().c_str(), 1);
#endif
    settings.red_orb_multiplier = 0.125f;
    Gow3Settings::Save();
    settings.red_orb_multiplier = 1;
    Gow3Settings::Load();
    assert(settings.red_orb_multiplier == 0.125f);
    assert(std::filesystem::remove(config));
#ifdef _WIN32
    VirtualFree(stub, 0, MEM_RELEASE);
#else
    munmap(stub, 4096);
#endif
    std::puts("orb-multiplier-test: OK (gains, spending, fractions, signatures, executable hook)");
}
