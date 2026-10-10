/* SPDX-License-Identifier: GPL-2.0-or-later */
#ifndef GOW3_DAMAGE_HOOK_H
#define GOW3_DAMAGE_HOOK_H
#include <stdint.h>
#include <string.h>

/* CUSA01623 v01.02, image offset 0x503f3, inside the damage routine: the actor's health before
 * the hit is at [rbp-0x5c], the damage at [rbp-0x60]; the game then clamps the difference to
 * [0, maximum] and stores it (0x5040e, the infinite-health cheat's site). r13 is the actor and
 * +0x298 is 0 for the player. rax, xmm1 and the flags are overwritten before they are read. */
enum { GOW3_DAMAGE_HOOK_OFFSET = 0x503f3, GOW3_DAMAGE_HOOK_LENGTH = 10 };
static const unsigned char gow3_damage_original[GOW3_DAMAGE_HOOK_LENGTH] = {
    0xc5,0xfa,0x10,0x45,0xa4,  /* vmovss xmm0, [rbp-0x5c] */
    0xc5,0xfa,0x5c,0x45,0xa0   /* vsubss xmm0, xmm0, [rbp-0x60] */
};

static inline int gow3_damage_signature_match(const unsigned char *code, uint64_t size) {
    return size >= GOW3_DAMAGE_HOOK_LENGTH && !memcmp(code, gow3_damage_original, GOW3_DAMAGE_HOOK_LENGTH);
}

/* Damage above zero is multiplied by *taken (player) or *dealt (any other actor). */
static inline unsigned gow3_damage_build_stub(unsigned char *out, uintptr_t taken, uintptr_t dealt,
                                              uintptr_t resume) {
    static const unsigned char body[] = {
        0x48,0xb8, 0,0,0,0,0,0,0,0,          /* movabs rax, taken */
        0x41,0x80,0xbd,0x98,0x02,0,0, 0x00,  /* cmp byte [r13+0x298], 0 */
        0x74,0x0a,                           /* je +10 (player) */
        0x48,0xb8, 0,0,0,0,0,0,0,0,          /* movabs rax, dealt */
        0xc5,0xfa,0x10,0x4d,0xa0,            /* vmovss xmm1, [rbp-0x60] */
        0xc5,0xf8,0x57,0xc0,                 /* vxorps xmm0, xmm0, xmm0 */
        0xc5,0xf8,0x2e,0xc8,                 /* vucomiss xmm1, xmm0 */
        0x76,0x09,                           /* jbe +9 (no damage, or NaN) */
        0xc5,0xf2,0x59,0x08,                 /* vmulss xmm1, xmm1, [rax] */
        0xc5,0xfa,0x11,0x4d,0xa0,            /* vmovss [rbp-0x60], xmm1 */
    };
    static const unsigned char jump[] = {0xff,0x25,0,0,0,0}; /* jmp [rip+0] */
    unsigned n = 0;
    memcpy(out, body, sizeof(body));
    memcpy(out + 2, &taken, 8);
    memcpy(out + 22, &dealt, 8);
    n += sizeof(body);
    memcpy(out + n, gow3_damage_original, GOW3_DAMAGE_HOOK_LENGTH);
    n += GOW3_DAMAGE_HOOK_LENGTH;
    memcpy(out + n, jump, sizeof(jump));
    n += sizeof(jump);
    memcpy(out + n, &resume, 8);
    return n + 8;
}
#endif
