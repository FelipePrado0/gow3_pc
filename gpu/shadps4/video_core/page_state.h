// SPDX-FileCopyrightText: Copyright 2024 shadPS4 Emulator Project
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include "common/assert.h"
#include "common/types.h"
#include "core/address_space.h"

namespace VideoCore {

struct PageState {
    u8 num_write_watchers;
    u8 num_read_watchers;

    Core::MemoryPermission WritePerm() const noexcept {
        return num_write_watchers == 0 ? Core::MemoryPermission::Write
                                       : Core::MemoryPermission::None;
    }

    Core::MemoryPermission ReadPerm() const noexcept {
        return num_read_watchers == 0 ? Core::MemoryPermission::Read
                                      : Core::MemoryPermission::None;
    }

    Core::MemoryPermission Perms() const noexcept {
        // A pending readback must block writes too, until its GPU version reaches RAM.
        return num_read_watchers != 0 ? Core::MemoryPermission::None
                                     : ReadPerm() | WritePerm();
    }

    template <s32 delta, bool is_read>
    u8 AddDelta() {
        if constexpr (is_read) {
            if constexpr (delta == 1) {
                return ++num_read_watchers;
            } else if (delta == -1) {
                ASSERT_MSG(num_read_watchers > 0, "Not enough watchers");
                return --num_read_watchers;
            } else {
                return num_read_watchers;
            }
        } else {
            if constexpr (delta == 1) {
                return ++num_write_watchers;
            } else if (delta == -1) {
                ASSERT_MSG(num_write_watchers > 0, "Not enough watchers");
                return --num_write_watchers;
            } else {
                return num_write_watchers;
            }
        }
    }
};

} // namespace VideoCore
