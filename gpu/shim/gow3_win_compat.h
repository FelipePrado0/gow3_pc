// SPDX-License-Identifier: GPL-2.0-or-later
// gow3 (Windows): small POSIX helpers the renderer uses for diagnostics and thread
// identity. Force-included into the GPU library on Windows only (CMakeLists.txt); kept free
// of <windows.h> so its macros do not reach every translation unit.
#pragma once
#ifdef _WIN32
#ifdef __cplusplus
extern "C" {
#endif
__declspec(dllimport) unsigned long __stdcall GetCurrentThreadId(void);
static inline int gettid(void) { return (int)GetCurrentThreadId(); }
#ifdef __cplusplus
}
#endif
#endif
