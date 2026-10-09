// SPDX-License-Identifier: GPL-2.0-or-later
#pragma once
#include <algorithm>
#include <cmath>

namespace Gow3Orbs {
inline float ClampMultiplier(float value) {
    return std::isfinite(value) ? std::clamp(value, 0.1f, 100.0f) : 1.0f;
}
inline float Scale(float balance, float requested, float multiplier) {
    if (!std::isfinite(balance) || !std::isfinite(requested) || requested <= balance) {
        return requested;
    }
    // The game stores fractional orbs as a float; preserve them rather than rounding each pickup.
    return float(double(balance) + (double(requested) - balance) * ClampMultiplier(multiplier));
}
}
