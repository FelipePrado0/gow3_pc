// Runs the QTE stubs (src/qte_hook.h) as the game would call the hooked functions: the update
// stub counts frames, the rotation stub answers 1 once after the pad set pass. With the prepared
// eboot.elf as argument it also finds both sites and the state block.
#include <cassert>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <fstream>
#include <iterator>
#include <vector>
#include <windows.h>
#include "../src/qte_hook.h"

// The stubs replay the functions' first instructions, then jump here: undo them and return what
// the game's own function would (0 from the rotation check: not finished yet).
extern "C" void qte_update_return();
extern "C" void qte_rotation_return();
asm(".globl qte_update_return\nqte_update_return:\n    pop %r15\n    pop %rbp\n    ret\n"
    ".globl qte_rotation_return\nqte_rotation_return:\n    pop %r14\n    pop %rbp\n    xor %eax, %eax\n    ret\n");

static void call_update(unsigned char* stub) {
    asm volatile("call *%0" : : "r"(stub) : "rax", "memory", "cc");
}

static uint32_t call_rotation(unsigned char* stub) {
    uint32_t result;
    asm volatile("call *%1" : "=a"(result) : "r"(stub) : "memory", "cc");
    return result;
}

int main(int argc, char** argv) {
    alignas(64) static Gow3QteState state;
    auto* code = static_cast<unsigned char*>(
        VirtualAlloc(nullptr, 4096, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE));
    assert(code);
    unsigned char* update = code;
    unsigned char* rotation = code + 128;
    assert(gow3_qte_build_update_stub(update, reinterpret_cast<uintptr_t>(&state.heartbeat),
                                      reinterpret_cast<uintptr_t>(&qte_update_return)) < 128);
    assert(gow3_qte_build_rotation_stub(rotation, reinterpret_cast<uintptr_t>(&state.pass),
                                        reinterpret_cast<uintptr_t>(&qte_rotation_return)) < 128);
    FlushInstructionCache(GetCurrentProcess(), code, 4096);

    // A frame of a prompt: the count moves, the registers the function saves come back intact.
    call_update(update);
    call_update(update);
    assert(state.heartbeat == 2);

    // Rotation: the game's own answer until the pad sets pass, then 1 exactly once.
    assert(call_rotation(rotation) == 0);
    state.pass = 1;
    assert(call_rotation(rotation) == 1);
    assert(state.pass == 0);
    assert(call_rotation(rotation) == 0);

    // The state block from the rotation check's lea r8,[rip+disp].
    unsigned char lea[GOW3_QTE_BLOCK_NEXT] = {};
    const int32_t displacement = 0x33940d;
    std::memcpy(lea + GOW3_QTE_BLOCK_DISPLACEMENT, &displacement, 4);
    assert(gow3_qte_block(lea, 0x12c280) == 0x4656b0);

    if (argc > 1) {
        // The code segment starts at file offset 0x4000 (image offset 0).
        std::ifstream file(argv[1], std::ios::binary);
        std::vector<unsigned char> data((std::istreambuf_iterator<char>(file)), {});
        assert(data.size() > 0x4000);
        const unsigned char* image = data.data() + 0x4000;
        const uint64_t size = data.size() - 0x4000;
        const uint64_t update_at = gow3_signature_find(image, size, &gow3_qte_update_site);
        const uint64_t rotation_at = gow3_signature_find(image, size, &gow3_qte_rotation_site);
        assert(update_at != UINT64_MAX && rotation_at != UINT64_MAX);
        assert(!std::memcmp(image + update_at, gow3_qte_update_original, GOW3_QTE_HOOK_LENGTH));
        assert(!std::memcmp(image + rotation_at, gow3_qte_rotation_original, GOW3_QTE_HOOK_LENGTH));
        const uint64_t block = gow3_qte_block(image + rotation_at, rotation_at);
        assert(block > update_at && block % 8 == 0);
        std::printf("QTE sites: update 0x%llx, rotation 0x%llx, state block 0x%llx\n",
                    static_cast<unsigned long long>(update_at), static_cast<unsigned long long>(rotation_at),
                    static_cast<unsigned long long>(block));
    }
    VirtualFree(code, 0, MEM_RELEASE);
    std::puts("PASS: QTE hooks");
}
