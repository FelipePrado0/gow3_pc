/* SPDX-License-Identifier: GPL-2.0-or-later */
#ifndef GOW3_ORB_HOOK_H
#define GOW3_ORB_HOOK_H
#include <stdint.h>
#include <string.h>

static const uint32_t gow3_orb_calls[] = {0x6a775, 0x6fa33, 0x6fbd7};
static const unsigned char gow3_orb_call_bytes[][5] = {
    {0xe8, 0x76, 0x02, 0x00, 0x00},
    {0xe8, 0xb8, 0xaf, 0xff, 0xff},
    {0xe8, 0x14, 0xae, 0xff, 0xff}
};
static inline void gow3_orb_build_stub(unsigned char code[26], uintptr_t helper, uintptr_t setter) {
    static const unsigned char pattern[26] = {
        0x57,0x48,0xb8,0,0,0,0,0,0,0,0,0xff,0xd0,0x5f,
        0x48,0xb8,0,0,0,0,0,0,0,0,0xff,0xe0
    };
    memcpy(code, pattern, 26);
    memcpy(code + 3, &helper, 8);
    memcpy(code + 16, &setter, 8);
}
static inline int gow3_orb_signatures_match(const unsigned char* image, uint64_t size) {
    static const unsigned char setter[] = {
        0x55,0x48,0x89,0xe5,0xc5,0xfa,0x5d,0x05,0x20,0xa4,0x28,0x00,
        0xc5,0xfa,0x11,0x87,0xe8,0x01,0x00,0x00
    };
    if (size < 0x6fbdc || memcmp(image + 0x6a9f0, setter, sizeof(setter))) return 0;
    for (unsigned i = 0; i < 3; ++i)
        if (memcmp(image + gow3_orb_calls[i], gow3_orb_call_bytes[i], 5)) return 0;
    return 1;
}
#endif
