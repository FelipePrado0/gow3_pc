// SPDX-License-Identifier: GPL-2.0-or-later
#include "gow3_actors.h"
#include "gow3_settings.h"
#include <cstdint>
#include "../../src/actor_hook.h"

alignas(64) Gow3ActorSlot gow3_actor_slots[2];

extern "C" uintptr_t gow3_actor_slots_address(void) {
    return reinterpret_cast<uintptr_t>(gow3_actor_slots);
}

extern "C" void gow3_actor_status(int supported) {
    Gow3Settings::Get().actor_watch_supported = supported != 0;
}
