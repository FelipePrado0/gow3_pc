/* C interface of the GPU library (gpu/): shadPS4's Liverpool/Vulkan video core,
 * GnmDriver, VideoOut and kernel event queues, adapted to the native loader. */
#ifndef GOW3GPU_H
#define GOW3GPU_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
typedef struct {
    const char *title;          /* window title */
    const char *serial;         /* CUSA id, names the pipeline cache */
    const char *user_dir;       /* pipeline cache/logs directory */
    uint32_t sdk_version;       /* from the eboot's procparam */
    uint32_t psf_attributes;    /* param.sfo ATTRIBUTE */
    int32_t width, height;      /* initial window size */
} Gow3GpuConfig;
/* Registers kernel event queues (needed with or without graphics). */
void gow3gpu_register_kernel(void);
/* Creates window, Vulkan device, presenter and GPU command processor. */
int gow3gpu_init(const Gow3GpuConfig *config);
/* Function for an imported NID ("NID#lib#mod"), or 0 when the GPU library does not provide it. */
uintptr_t gow3gpu_resolve(const char *scoped_nid);
/* Called first by the loader's SIGSEGV handler: 1 when a GPU page-tracking fault was handled. */
int gow3gpu_handle_fault(void *ucontext, void *address);
/* GOW3_WRITE_LOG=1: prints the logged GPU-side writes to guest memory near the fault. */
void gow3gpu_dump_guest_writes(void *ucontext);
/* Keyboard text entry through the game window (IME dialog). begin returns 0 when
 * no window exists; poll returns 0 typing, 1 confirmed, 2 cancelled (UTF-8 text). */
int gow3gpu_text_input_begin(const char *initial_utf8, const char *prompt_utf8);
int gow3gpu_text_input_poll(char *out_utf8, uint64_t size);
/* A list for the player to choose from, drawn over the game (save list). images: a PNG path per
 * item drawn beside it (NULL array or entry: none). begin: 0 when no window exists.
 * poll: -1 while open, -2 cancelled, else the chosen index. */
int gow3gpu_choice_begin(const char *title, const char *const *items, const char *const *images, int count, int focus);
int gow3gpu_choice_poll(void);
/* 1 while the in-game settings menu is open: the game's pad input is held neutral. */
int gow3gpu_overlay_captures_input(void);
/* Controller vibration strength set in the menu, 0 (off) to 100 (the game's own). */
int gow3gpu_vibration_percent(void);
/* The video decoder handed the game a picture: a cutscene is on screen (the presenter draws
 * black bars outside its 16:9 picture when the render resolution is not 16:9). */
void gow3gpu_movie_picture(void);
/* Number of symbols registered by the vendored libraries (diagnostics). */
unsigned gow3gpu_symbol_count(void);
/* Trophy notice at the top-right of the game window (any thread). title: the trophy's name,
 * detail: its description (null or empty: none), png: its icon (PNG bytes; null: a drawn medal). */
void gow3gpu_toast_trophy(const char *title, const char *detail, const unsigned char *png, uint64_t png_size);
void gow3gpu_toast_icon(const char *text, const unsigned char *png, uint64_t png_size);
void gow3gpu_toast(const char *text);
/* The in-game trophy list (F8 / R3+R2) and the notice labels, handed over once by the runtime.
 * labels: texts of the window and of the notice (null or empty: English). items: the trophies in
 * the order of the list (their strings and png stay valid). unlocked: four 32-bit words, bit
 * id % 32 of word id / 32 set for an unlocked trophy, read live. */
typedef struct {
    const char *heading, *notice, *status_unlocked, *status_locked;
    const char *filter_all, *filter_unlocked, *filter_locked;
    const char *counter; /* "{} of {} unlocked" */
    const char *hint;    /* the keys */
} Gow3GpuTrophyLabels;
typedef struct {
    int id;
    const char *name, *detail;
    const unsigned char *png;
    uint64_t png_size;
} Gow3GpuTrophyInfo;
void gow3gpu_trophy_setup(const Gow3GpuTrophyLabels *labels, const Gow3GpuTrophyInfo *items, int count,
                          const volatile uint32_t *unlocked);
#ifdef __cplusplus
}
#endif
#endif
