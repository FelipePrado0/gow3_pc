// SPDX-License-Identifier: GPL-2.0-or-later
// gow3 (macOS): small POSIX helpers the renderer uses for diagnostics and thread identity.
// Force-included into the GPU library on macOS (CMakeLists.txt).
#pragma once
#ifdef __APPLE__
#include <stdint.h>
#include <pthread.h>

#ifdef __cplusplus
#include "common/polyfill_thread.h"
extern "C" {
#endif

static inline int gettid(void) {
    uint64_t tid = 0;
    pthread_threadid_np(NULL, &tid);
    return (int)tid;
}

#ifdef __cplusplus
}
#endif
#endif
