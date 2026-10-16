/* SPDX-License-Identifier: GPL-2.0-or-later */
#ifndef GOW3_QTE_HOOK_H
#define GOW3_QTE_HOOK_H
#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include "signature.h"

/* The game's QTE prompts ("minigame buttons"). Offsets as found in a v01.02 build; builds are found
 * by signature. 0x12c670 starts a prompt: it stores the prompt type at +0x10 of a state block
 * (0 triangle, 1 circle, 2 cross, 3 square, 4 Up180CW, 5 Up180CCW, 6 Down180CW, 7 Down180CCW,
 * 8 360CW, 9 360CCW) and makes 0x12c4a0 the object's update, which runs every frame while the
 * prompt is up. For a rotation, that update asks 0x12c280 whether the stick finished the turn
 * (1: passed, then the game plays its own success).
 * Update hook: counts the frames (a prompt is up while the count moves). Rotation hook: answers 1
 * once when the port has set pass. Both sites are the functions' first three instructions
 * (push rbp; mov rbp,rsp; push r15 or r14), where rax and the flags are free. The state block's
 * address is read from the rotation check's lea r8,[rip+disp] (site +0x1c). */
enum { GOW3_QTE_HOOK_LENGTH = 6, GOW3_QTE_TYPE_OFFSET = 0x10,
       GOW3_QTE_BLOCK_DISPLACEMENT = 0x1f, GOW3_QTE_BLOCK_NEXT = 0x23 };
static const unsigned char gow3_qte_update_original[GOW3_QTE_HOOK_LENGTH] = {
    0x55, 0x48,0x89,0xe5, 0x41,0x57  /* push rbp; mov rbp,rsp; push r15 */
};
static const unsigned char gow3_qte_rotation_original[GOW3_QTE_HOOK_LENGTH] = {
    0x55, 0x48,0x89,0xe5, 0x41,0x56  /* push rbp; mov rbp,rsp; push r14 */
};
static const Gow3Signature gow3_qte_update_site = {
    "55 48 89 e5 41 57 41 56 41 55 41 54 53 50 49 89 ff 49 8b 87 b0 00 00 00 a8 07 0f 85 ?? ?? ?? ?? 48 8d 0d", 0};
static const Gow3Signature gow3_qte_rotation_site = {
    "55 48 89 e5 41 56 53 49 89 fe c5 fa 10 05 ?? ?? ?? ?? 31 ff 4c 89 f6 e8 ?? ?? ?? ?? 4c 8d 05 ?? ?? ?? ?? "
    "49 8b 58 10 0f b7 d3", 0};

/* Shared by the stubs (game thread) and the pad (runtime_pad.c). block: the game's state block,
 * 0 while the hooks are not installed. */
typedef struct {
    uint32_t heartbeat;  /* update calls */
    uint8_t pass;        /* set by the pad; the rotation stub answers 1 once and clears it */
    uint8_t pad[3];
    uint64_t block;
} Gow3QteState;
#ifdef __cplusplus
static_assert(offsetof(Gow3QteState, pass) == 4, "stubs address the fields directly");
#else
_Static_assert(offsetof(Gow3QteState, pass) == 4, "stubs address the fields directly");
#endif

/* The state block's image offset, read from the rotation check at its signature site. */
static inline uint64_t gow3_qte_block(const unsigned char *code, uint64_t site) {
    int32_t displacement;
    memcpy(&displacement, code + GOW3_QTE_BLOCK_DISPLACEMENT, 4);
    return site + GOW3_QTE_BLOCK_NEXT + (int64_t)displacement;
}

static inline unsigned gow3_qte_finish_stub(unsigned char *out, unsigned n, const unsigned char *original,
                                            uintptr_t resume) {
    static const unsigned char jump[] = {0xff,0x25,0,0,0,0}; /* jmp [rip+0] */
    memcpy(out + n, original, GOW3_QTE_HOOK_LENGTH);
    n += GOW3_QTE_HOOK_LENGTH;
    memcpy(out + n, jump, sizeof(jump));
    n += sizeof(jump);
    memcpy(out + n, &resume, 8);
    return n + 8;
}

static inline unsigned gow3_qte_build_update_stub(unsigned char *out, uintptr_t heartbeat, uintptr_t resume) {
    static const unsigned char body[] = {
        0x48,0xb8, 0,0,0,0,0,0,0,0,  /* movabs rax, heartbeat */
        0xf0,0xff,0x00,              /* lock inc dword [rax] */
    };
    memcpy(out, body, sizeof(body));
    memcpy(out + 2, &heartbeat, 8);
    return gow3_qte_finish_stub(out, sizeof(body), gow3_qte_update_original, resume);
}

static inline unsigned gow3_qte_build_rotation_stub(unsigned char *out, uintptr_t pass, uintptr_t resume) {
    static const unsigned char body[] = {
        0x48,0xb8, 0,0,0,0,0,0,0,0,  /* movabs rax, pass */
        0x80,0x38,0x00,              /* cmp byte [rax], 0 */
        0x74,0x09,                   /* je original */
        0xc6,0x00,0x00,              /* mov byte [rax], 0 */
        0xb8,0x01,0x00,0x00,0x00,    /* mov eax, 1 */
        0xc3,                        /* ret: the turn is finished */
    };
    memcpy(out, body, sizeof(body));
    memcpy(out + 2, &pass, 8);
    return gow3_qte_finish_stub(out, sizeof(body), gow3_qte_rotation_original, resume);
}
#endif
