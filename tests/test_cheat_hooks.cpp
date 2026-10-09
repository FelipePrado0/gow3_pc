#include <cassert>
#include "../src/cheat_hook.h"
#include <array>
#include <atomic>
#include <cstdio>
#include <vector>
#ifdef _WIN32
#include <windows.h>
#else
#include <sys/mman.h>
#endif

int main(int argc, char** argv) {
    unsigned char code[256]{};
    std::vector<unsigned char> image;
    if (argc > 1) {
        FILE* file = std::fopen(argv[1], "rb");
        assert(file && !std::fseek(file, 0, SEEK_END));
        const auto size = std::ftell(file);
        assert(size > 0x4000);
        image.resize(size - 0x4000);
        assert(!std::fseek(file, 0x4000, SEEK_SET));
        assert(std::fread(image.data(), 1, image.size(), file) == image.size());
        std::fclose(file);
    }
    for (unsigned i = 0; i < GOW3_CHEAT_HOOK_COUNT; ++i) {
        const auto& hook = gow3_cheat_hooks[i];
        assert(gow3_cheat_signature_match(&hook, hook.original, hook.length));
        code[0] = 0;
        assert(!gow3_cheat_signature_match(&hook, code, hook.length));
        assert(!gow3_cheat_signature_match(&hook, hook.original, hook.length - 1));
        assert(gow3_cheat_build_stub(code, &hook, 0x12345678, 0x87654321) < sizeof(code));
        if (!image.empty()) {
            assert(hook.offset + hook.length <= image.size());
            assert(gow3_cheat_signature_match(&hook, image.data() + hook.offset, image.size() - hook.offset));
        }
#ifdef _WIN32
        auto* executable = static_cast<unsigned char*>(VirtualAlloc(nullptr, 4096, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE));
#else
        auto* executable = static_cast<unsigned char*>(mmap(nullptr, 4096, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0));
        assert(executable != MAP_FAILED);
#endif
        assert(executable);
        // Emulate each game's register convention, including its flags and a live RAX value.
        const unsigned char entry[] = {0x53,0x41,0x55,0x48,0x89,0xfb,0x49,0x89,0xfd,
            0xc5,0xf8,0x28,0xc8,0x48,0xb8,0x78,0x56,0x34,0x12,0,0,0,0,0xf9,
            0xff,0x25,0,0,0,0};
        memcpy(executable, entry, sizeof(entry));
        uintptr_t target = reinterpret_cast<uintptr_t>(executable + 64);
        memcpy(executable + sizeof(entry), &target, 8);
        std::atomic<bool> flag{false};
        assert(gow3_cheat_build_stub(executable + 64, &hook, reinterpret_cast<uintptr_t>(&flag),
                                    reinterpret_cast<uintptr_t>(executable + 256)) < 192);
        const unsigned char exit[] = {0x0f,0x92,0x87,0,3,0,0,
            0x48,0x89,0x87,8,3,0,0,0x41,0x5d,0x5b,0xc3};
        memcpy(executable + 256, exit, sizeof(exit));
#ifdef _WIN32
        DWORD previous;
        assert(VirtualProtect(executable, 4096, PAGE_EXECUTE_READ, &previous));
        FlushInstructionCache(GetCurrentProcess(), executable, 4096);
#else
        assert(!mprotect(executable, 4096, PROT_READ | PROT_EXEC));
#endif
        using Execute = void (__attribute__((sysv_abi)) *)(void*, float);
        const auto execute = reinterpret_cast<Execute>(executable);
        std::array<unsigned char, 0x400> player{};
        const unsigned offsets[] = {0x1cc,0x1d0,0x1d4,0x1d8,0x1e8};
        const unsigned offset = offsets[hook.id];
        const float original = 100, health_max = 250;
        memcpy(player.data() + offset, &original, 4);
        memcpy(player.data() + 0x1d0, &health_max, 4);
        execute(player.data(), 5);
        float result;
        memcpy(&result, player.data() + offset, 4);
        assert(result == 5 && player[0x300] == 1);
        uint64_t preserved;
        memcpy(&preserved, player.data() + 0x308, 8);
        assert(preserved == 0x12345678);
        memcpy(player.data() + offset, &original, 4);
        if (hook.id == GOW3_CHEAT_HEALTH) memcpy(player.data() + 0x1d0, &health_max, 4);
        flag.store(true);
        execute(player.data(), 5);
        memcpy(&result, player.data() + offset, 4);
        const float expected[] = {250,100,100,1000000000,999999};
        assert(result == expected[hook.id] && player[0x300] == 1);
        memcpy(&preserved, player.data() + 0x308, 8);
        assert(preserved == 0x12345678);
        if (hook.id == GOW3_CHEAT_HEALTH) {
            player[0x298] = 1;
            execute(player.data(), 7);
            memcpy(&result, player.data() + offset, 4);
            assert(result == 7);
        }
        flag.store(false);
        execute(player.data(), 3);
        memcpy(&result, player.data() + offset, 4);
        assert(result == 3);
#ifdef _WIN32
        VirtualFree(executable, 0, MEM_RELEASE);
#else
        munmap(executable, 4096);
#endif
    }
    std::puts("cheat-hooks-test: OK (6 hooks, live toggles, registers, flags, signatures, non-player health)");
}
