// Damage and green/blue/gold orb multipliers (src/damage_hook.h, src/orb_hook.h).
#include <cassert>
#include <cstdio>
#include <cstring>
#include <fstream>
#include <limits>
#include <sstream>
#include <string>
#include <vector>
#include "../src/damage_hook.h"
#include "../src/orb_hook.h"
#include "gow3_settings.h"
#ifdef _WIN32
#include <windows.h>
#else
#include <sys/mman.h>
#endif

static unsigned char* Executable() {
#ifdef _WIN32
    auto* code = static_cast<unsigned char*>(VirtualAlloc(nullptr, 4096, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE));
#else
    auto* code = static_cast<unsigned char*>(mmap(nullptr, 4096, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0));
    assert(code != MAP_FAILED);
#endif
    assert(code);
    return code;
}

static void Seal(unsigned char* code) {
#ifdef _WIN32
    DWORD previous;
    assert(VirtualProtect(code, 4096, PAGE_EXECUTE_READ, &previous));
    FlushInstructionCache(GetCurrentProcess(), code, 4096);
#else
    assert(!mprotect(code, 4096, PROT_READ | PROT_EXEC));
#endif
}

static uintptr_t Address(const void* p) {
    return reinterpret_cast<uintptr_t>(p);
}

// Runs the damage stub as the game would: r13 the actor, rbp the routine's frame. Returns the
// health the game would compute (xmm0 at the resume point) and the damage left in the frame.
static float Damage(bool player, float health, float damage, float taken, float dealt, float* frame_damage) {
    unsigned char* code = Executable();
    const unsigned char entry[] = {0x55, 0x41,0x55, 0x49,0x89,0xfd, 0x48,0x89,0xf5, 0xff,0x25,0,0,0,0};
    std::memcpy(code, entry, sizeof(entry));
    const uintptr_t stub = Address(code + 64), exit = Address(code + 512);
    std::memcpy(code + sizeof(entry), &stub, 8);
    assert(gow3_damage_build_stub(code + 64, Address(&taken), Address(&dealt), exit) < 448);
    const unsigned char leave[] = {0xc5,0xfa,0x11,0x45,0x98, 0x41,0x5d, 0x5d, 0xc3};
    std::memcpy(code + 512, leave, sizeof(leave));
    Seal(code);
    std::vector<unsigned char> actor(0x400);
    actor[0x298] = player ? 0 : 1;
    float frame[32]{};
    float* top = frame + 32;
    std::memcpy(reinterpret_cast<unsigned char*>(top) - 0x5c, &health, 4);
    std::memcpy(reinterpret_cast<unsigned char*>(top) - 0x60, &damage, 4);
    reinterpret_cast<void(__attribute__((sysv_abi))*)(void*, void*)>(code)(actor.data(), top);
    float result;
    std::memcpy(&result, reinterpret_cast<unsigned char*>(top) - 0x68, 4);
    std::memcpy(frame_damage, reinterpret_cast<unsigned char*>(top) - 0x60, 4);
    return result;
}

struct Amounts {
    float health, magic, rage;
};

// Runs the pickup stub with the routine's arguments; the resume point stores them and undoes
// the prologue the stub replayed.
static Amounts Pickup(Amounts in, float green, float blue, float gold) {
    unsigned char* code = Executable();
    assert(gow3_orb_pickup_build_stub(code, Address(&green), Address(&blue), Address(&gold),
                                      Address(code + 512)) < 512);
    const unsigned char leave[] = {0xc5,0xfa,0x11,0x07, 0xc5,0xfa,0x11,0x4f,0x04, 0xc5,0xfa,0x11,0x57,0x08,
                                   0x48,0x83,0xc4,0x18, 0x5b, 0x5d, 0xc3};
    std::memcpy(code + 512, leave, sizeof(leave));
    Seal(code);
    Amounts out{};
    reinterpret_cast<void(__attribute__((sysv_abi))*)(Amounts*, float, float, float)>(code)(
        &out, in.health, in.magic, in.rage);
    return out;
}

static std::string Read(const char* path) {
    std::ifstream in(path);
    std::stringstream s;
    s << in.rdbuf();
    return s.str();
}

int main(int argc, char** argv) {
    assert(gow3_damage_signature_match(gow3_damage_original, GOW3_DAMAGE_HOOK_LENGTH));
    assert(!gow3_damage_signature_match(gow3_damage_original, GOW3_DAMAGE_HOOK_LENGTH - 1));
    assert(gow3_orb_pickup_signature_match(gow3_orb_pickup_original, GOW3_ORB_PICKUP_LENGTH));
    if (argc > 1) {
        std::ifstream file(argv[1], std::ios::binary);
        std::vector<unsigned char> image((std::istreambuf_iterator<char>(file)), {});
        assert(image.size() > 0x4000 + GOW3_ORB_PICKUP_OFFSET + GOW3_ORB_PICKUP_LENGTH);
        assert(gow3_damage_signature_match(image.data() + 0x4000 + GOW3_DAMAGE_HOOK_OFFSET, GOW3_DAMAGE_HOOK_LENGTH));
        assert(gow3_orb_pickup_signature_match(image.data() + 0x4000 + GOW3_ORB_PICKUP_OFFSET, GOW3_ORB_PICKUP_LENGTH));
    }

    float left;
    assert(Damage(true, 100, 10, 2, 5, &left) == 80 && left == 20);    // player: taken
    assert(Damage(false, 100, 10, 2, 5, &left) == 50 && left == 50);   // enemy: dealt
    assert(Damage(false, 100, 10, 0.5f, 0.1f, &left) == 99 && left == 1);
    assert(Damage(true, 100, -10, 2, 5, &left) == 110 && left == -10); // healing stays
    assert(Damage(true, 100, 0, 2, 5, &left) == 100 && left == 0);
    const float nan = std::numeric_limits<float>::quiet_NaN();
    Damage(false, 100, nan, 2, 5, &left);
    assert(left != left);

    Amounts a = Pickup({10, 20, 30}, 2, 3, 0.5f);
    assert(a.health == 20 && a.magic == 60 && a.rage == 15);
    a = Pickup({0, -5, 4}, 2, 3, 100);
    assert(a.health == 0 && a.magic == -5 && a.rage == 400);

    const char* path = "stat-multipliers-test.ini";
    {
        std::ofstream out(path);
        out << "damage_dealt=2.5\ndamage_taken=0.5\ngreen_orb_multiplier=3\n"
               "blue_orb_multiplier=1000\ngold_orb_multiplier=0\n";
    }
#ifdef _WIN32
    _putenv_s("GOW3_CONFIG", path);
#else
    setenv("GOW3_CONFIG", path, 1);
#endif
    auto& v = Gow3Settings::Get();
    Gow3Settings::Load();
    assert(v.damage_dealt == 2.5f && v.damage_taken == 0.5f && v.green_orb_multiplier == 3.0f);
    assert(v.blue_orb_multiplier == 100.0f && v.gold_orb_multiplier == 0.1f);
    v.gold_orb_multiplier = 4.0f;
    Gow3Settings::Save();
    const std::string saved = Read(path);
    for (const char* line : {"damage_dealt=2.500\n", "damage_taken=0.500\n", "green_orb_multiplier=3.000\n",
                             "blue_orb_multiplier=100.000\n", "gold_orb_multiplier=4.000\n"}) {
        assert(saved.find(line) != std::string::npos);
    }
    std::remove(path);
    std::puts("PASS: stat multipliers");
}
