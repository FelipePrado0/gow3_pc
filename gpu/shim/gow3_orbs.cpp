// SPDX-License-Identifier: GPL-2.0-or-later
#include "gow3_orbs.h"
#include "gow3_settings.h"
#include <cstring>
#include <cstdint>
#include "../../src/cheat_hook.h"

extern "C" float __attribute__((sysv_abi)) gow3_orbs_gain(void* player, float requested) {
    float balance;
    std::memcpy(&balance, static_cast<unsigned char*>(player) + 0x1e8, sizeof(balance));
    const auto& settings = Gow3Settings::Get();
    if ((settings.cheats_supported.load() & (1u << GOW3_CHEAT_ORBS)) &&
        settings.cheats[GOW3_CHEAT_ORBS].load()) {
        return requested;
    }
    return Gow3Orbs::Scale(balance, requested, Gow3Settings::Get().red_orb_multiplier.load());
}

extern "C" uintptr_t gow3_cheat_flag(unsigned id) {
    static_assert(sizeof(std::atomic<bool>) == 1 && std::atomic<bool>::is_always_lock_free);
    return id < GOW3_CHEAT_COUNT ? reinterpret_cast<uintptr_t>(&Gow3Settings::Get().cheats[id]) : 0;
}

extern "C" void gow3_cheat_status(unsigned id) {
    if (id < GOW3_CHEAT_COUNT) {
        Gow3Settings::Get().cheats_supported.fetch_or(1u << id);
    }
}

extern "C" void gow3_orbs_status(int supported) {
    Gow3Settings::Get().red_orbs_supported = supported != 0;
}
