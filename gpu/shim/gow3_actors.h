// SPDX-License-Identifier: GPL-2.0-or-later
// gow3: health of the player and of the last actor hit, recorded by the damage hook
// (src/actor_hook.h); the overlay draws the enemy health bar from it.
#pragma once
#include <algorithm>
#include <cmath>

namespace Gow3Actors {
/// A recorded pair is shown only when it looks like health: finite, 0 <= health <= max.
inline bool Valid(float health, float max_health) {
    return std::isfinite(health) && std::isfinite(max_health) && max_health > 0.0f &&
           max_health < 1e7f && health >= 0.0f && health <= max_health * 1.001f;
}
inline float Fraction(float health, float max_health) {
    return Valid(health, max_health) ? std::clamp(health / max_health, 0.0f, 1.0f) : 0.0f;
}
}
