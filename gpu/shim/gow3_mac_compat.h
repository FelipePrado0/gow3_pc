// gow3 (macOS): Linux calls the vendored video core relies on. Included before every source
// of the GPU library (CMakeLists.txt), like gow3_win_compat.h on Windows.
#pragma once
#ifdef __APPLE__
#include <pthread.h>
#include <stdint.h>

// Numeric id of the calling thread, as Linux's gettid. macOS thread ids are 64-bit and never
// reused; the low 32 bits tell the threads of one process apart. Cached: the video core
// compares it on hot paths.
static inline int gettid(void) {
    static __thread int id;
    if (!id) {
        uint64_t value = 0;
        pthread_threadid_np(NULL, &value);
        id = (int)value;
    }
    return id;
}
#endif
