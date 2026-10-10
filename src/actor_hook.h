/* SPDX-License-Identifier: GPL-2.0-or-later */
#ifndef GOW3_ACTOR_HOOK_H
#define GOW3_ACTOR_HOOK_H
#include <stdint.h>
#include <string.h>

/* CUSA01623 v01.02, image offset 0x50417: the instruction after the damage routine stores an
 * actor's new health (vmovss [r13+0x1cc], xmm0 at 0x5040e, the infinite-health cheat's site).
 * r13 is the actor, health at +0x1cc, maximum at +0x1d0, and +0x298 is 0 for the player
 * (the infinite-health cheat refills only those). rax, rdi
 * and the flags are overwritten right after, so the stub may use them. */
enum { GOW3_ACTOR_HOOK_OFFSET = 0x50417, GOW3_ACTOR_HOOK_LENGTH = 7 };
static const unsigned char gow3_actor_original[GOW3_ACTOR_HOOK_LENGTH] = {
    0x49,0x8b,0x45,0x00,  /* mov rax, [r13] */
    0x4c,0x89,0xef        /* mov rdi, r13 */
};

/* What the stub records for the player (slots[0]) and the last other actor hit (slots[1]).
 * Written by the game thread, read by the overlay; seq changes on every hit. */
typedef struct {
    uint64_t actor;
    float health, max_health;
    uint32_t seq, pad;
    uint64_t reserved;
} Gow3ActorSlot;
_Static_assert(sizeof(Gow3ActorSlot) == 32, "stub uses a 32-byte stride");

static inline int gow3_actor_signature_match(const unsigned char *code, uint64_t size) {
    return size >= GOW3_ACTOR_HOOK_LENGTH && !memcmp(code, gow3_actor_original, GOW3_ACTOR_HOOK_LENGTH);
}

static inline unsigned gow3_actor_build_stub(unsigned char *out, uintptr_t slots, uintptr_t resume) {
    static const unsigned char body[] = {
        0x48,0xb8, 0,0,0,0,0,0,0,0,               /* movabs rax, slots */
        0x41,0x80,0xbd,0x98,0x02,0,0, 0x00,       /* cmp byte [r13+0x298], 0 */
        0x74,0x04,                                /* je +4 (player: slots[0]) */
        0x48,0x83,0xc0,0x20,                      /* add rax, 32 (other actor: slots[1]) */
        0x4c,0x89,0x28,                           /* mov [rax], r13 */
        0x41,0x8b,0xbd,0xcc,0x01,0,0,             /* mov edi, [r13+0x1cc] */
        0x89,0x78,0x08,                           /* mov [rax+8], edi */
        0x41,0x8b,0xbd,0xd0,0x01,0,0,             /* mov edi, [r13+0x1d0] */
        0x89,0x78,0x0c,                           /* mov [rax+12], edi */
        0xf0,0xff,0x40,0x10,                      /* lock inc dword [rax+16] */
    };
    static const unsigned char jump[] = {0xff,0x25,0,0,0,0}; /* jmp [rip+0] */
    unsigned n = 0;
    memcpy(out, body, sizeof(body));
    memcpy(out + 2, &slots, 8);
    n += sizeof(body);
    memcpy(out + n, gow3_actor_original, GOW3_ACTOR_HOOK_LENGTH);
    n += GOW3_ACTOR_HOOK_LENGTH;
    memcpy(out + n, jump, sizeof(jump));
    n += sizeof(jump);
    memcpy(out + n, &resume, 8);
    return n + 8;
}
#endif
