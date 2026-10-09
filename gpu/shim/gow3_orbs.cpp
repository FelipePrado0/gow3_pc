// SPDX-License-Identifier: GPL-2.0-or-later
#include "gow3_orbs.h"
#include "gow3_settings.h"
#include <cstring>
#include <cstdint>

extern "C" float __attribute__((sysv_abi)) gow3_orbs_gain(void* player, float requested) {
    float balance;
    std::memcpy(&balance, static_cast<unsigned char*>(player) + 0x1e8, sizeof(balance));
    return Gow3Orbs::Scale(balance, requested, Gow3Settings::Get().red_orb_multiplier.load());
}

extern "C" void gow3_orbs_status(int supported) {
    Gow3Settings::Get().red_orbs_supported = supported != 0;
}
