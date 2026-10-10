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

/// Damage (src/damage_hook.h): the multiplier the player's (taken) or another actor's hits use.
extern "C" uintptr_t gow3_damage_multiplier(int taken) {
    static_assert(sizeof(std::atomic<float>) == 4 && std::atomic<float>::is_always_lock_free);
    auto& s = Gow3Settings::Get();
    return reinterpret_cast<uintptr_t>(taken ? &s.damage_taken : &s.damage_dealt);
}

/// Orb pickup (src/orb_hook.h): 0 green, 1 blue, 2 gold.
extern "C" uintptr_t gow3_orb_pickup_multiplier(unsigned kind) {
    auto& s = Gow3Settings::Get();
    return reinterpret_cast<uintptr_t>(kind == 0 ? &s.green_orb_multiplier
                                       : kind == 1 ? &s.blue_orb_multiplier
                                                   : &s.gold_orb_multiplier);
}

extern "C" void gow3_stat_multipliers_status(int damage, int orb_pickup) {
    Gow3Settings::Get().damage_supported = damage != 0;
    Gow3Settings::Get().orb_pickup_supported = orb_pickup != 0;
}
