// Runs the menu interpreter's builtin-call stub (src/menu_hook.h): any other function replays the
// replaced instructions and resumes; function 0xfff goes to quit.
#include <cassert>
#include <cstdint>
#include "../src/menu_hook.h"
#include <windows.h>

// The stub ends with a jump to its resume address; here that is a `ret` back to the test.
extern "C" void menu_hook_return();
asm(".globl menu_hook_return\nmenu_hook_return:\n    ret\n");

static void* back[5];
static int quits;
static void quit() {
    ++quits;
    __builtin_longjmp(back, 1);  // no unwinding through the stub
}

// The interpreter frame: [rbp-0x2a0] is the provider table, rax the provider; rdi gets the entry.
alignas(16) static uint64_t frame[96];
static uint64_t providers[2] = {0x1111, 0x2222};

static uint64_t call(unsigned char* stub, uint32_t function, uint64_t provider) {
    uint64_t rdi;
    asm volatile("push %%rbp\n\tmov %%rbx, %%rbp\n\tcall *%%rcx\n\tpop %%rbp"
                 : "=D"(rdi), "+c"(stub), "+a"(provider), "+S"(function)
                 : "b"(frame + 96)
                 : "rdx", "r8", "r9", "r10", "r11", "memory", "cc");
    return rdi;
}

int main() {
    assert(gow3_menu_signature_match(gow3_menu_original, GOW3_MENU_HOOK_LENGTH));
    assert(!gow3_menu_signature_match(gow3_menu_original, GOW3_MENU_HOOK_LENGTH - 1));

    auto* stub = static_cast<unsigned char*>(
        VirtualAlloc(nullptr, 4096, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE));
    assert(stub);
    assert(gow3_menu_build_stub(stub, reinterpret_cast<uintptr_t>(&quit),
                                reinterpret_cast<uintptr_t>(&menu_hook_return)) < 256);
    frame[96 - 0x2a0 / 8] = reinterpret_cast<uint64_t>(providers);

    assert(call(stub, 0x5a, 1) == 0x2222);  // a game builtin: the original lookup
    assert(call(stub, 0xffe, 0) == 0x1111 && quits == 0);
    if (!__builtin_setjmp(back)) {
        call(stub, GOW3_MENU_EXIT_FUNCTION, 0);
        assert(!"quit returned");
    }
    assert(quits == 1);
    return 0;
}
