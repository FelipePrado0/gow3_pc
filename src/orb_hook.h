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

/* Orb pickup, image offset 0x6a480 (one caller, 0xbf496): rdi is the player's stats; xmm0
 * health (green), xmm1 magic (blue), xmm2 Rage of Sparta (gold), xmm3 red orbs (scaled by the
 * hooks above). The routine adds each positive amount and clamps it to the bar. The stub
 * replaces the prologue at the entry, where rax, xmm4 and the flags are free. */
enum { GOW3_ORB_PICKUP_OFFSET = 0x6a480, GOW3_ORB_PICKUP_LENGTH = 9 };
static const unsigned char gow3_orb_pickup_original[GOW3_ORB_PICKUP_LENGTH] = {
    0x55,                /* push rbp */
    0x48,0x89,0xe5,      /* mov rbp, rsp */
    0x53,                /* push rbx */
    0x48,0x83,0xec,0x18  /* sub rsp, 0x18 */
};

static inline int gow3_orb_pickup_signature_match(const unsigned char *code, uint64_t size) {
    return size >= GOW3_ORB_PICKUP_LENGTH && !memcmp(code, gow3_orb_pickup_original, GOW3_ORB_PICKUP_LENGTH);
}

/* Each amount above zero is multiplied by the float at green, blue or gold. */
static inline unsigned gow3_orb_pickup_build_stub(unsigned char *out, uintptr_t green, uintptr_t blue,
                                                  uintptr_t gold, uintptr_t resume) {
    static const unsigned char body[] = {
        0xc5,0xd8,0x57,0xe4,                 /* vxorps xmm4, xmm4, xmm4 */
        0x48,0xb8, 0,0,0,0,0,0,0,0,          /* movabs rax, green */
        0xc5,0xf8,0x2e,0xc4,                 /* vucomiss xmm0, xmm4 */
        0x76,0x04,                           /* jbe +4 */
        0xc5,0xfa,0x59,0x00,                 /* vmulss xmm0, xmm0, [rax] */
        0x48,0xb8, 0,0,0,0,0,0,0,0,          /* movabs rax, blue */
        0xc5,0xf8,0x2e,0xcc,                 /* vucomiss xmm1, xmm4 */
        0x76,0x04,                           /* jbe +4 */
        0xc5,0xf2,0x59,0x08,                 /* vmulss xmm1, xmm1, [rax] */
        0x48,0xb8, 0,0,0,0,0,0,0,0,          /* movabs rax, gold */
        0xc5,0xf8,0x2e,0xd4,                 /* vucomiss xmm2, xmm4 */
        0x76,0x04,                           /* jbe +4 */
        0xc5,0xea,0x59,0x10,                 /* vmulss xmm2, xmm2, [rax] */
    };
    static const unsigned char jump[] = {0xff,0x25,0,0,0,0}; /* jmp [rip+0] */
    unsigned n = 0;
    memcpy(out, body, sizeof(body));
    memcpy(out + 6, &green, 8);
    memcpy(out + 26, &blue, 8);
    memcpy(out + 46, &gold, 8);
    n += sizeof(body);
    memcpy(out + n, gow3_orb_pickup_original, GOW3_ORB_PICKUP_LENGTH);
    n += GOW3_ORB_PICKUP_LENGTH;
    memcpy(out + n, jump, sizeof(jump));
    n += sizeof(jump);
    memcpy(out + n, &resume, 8);
    return n + 8;
}
#endif
