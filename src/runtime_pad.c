/* libScePad on SDL3 gamepads, with a keyboard fallback. SDL events are pumped
 * by the window thread (gpu/shim/window.cpp); here state is only sampled.
 *
 * Default keyboard layout (GOW3_KEY_MAP changes the buttons, see apply_button_map):
 *   WASD left stick, arrow keys right stick, Space Cross, LShift Circle,
 *   E Square, Q Triangle, 1 L1, 3 R1, R L2, F R2, Z L3, C R3,
 *   Enter Options, Tab left touchpad (photo mode), Backspace right touchpad (pause menu),
 *   IJKL d-pad (I up, K down, J left, L right).
 * GOW3_KEY_MAP also takes the mouse: Mouse Left, Mouse Right, Mouse Middle, Mouse Back,
 * Mouse Forward, Wheel Up and Wheel Down (see mouse_names). */
#define _GNU_SOURCE
#include "runtime.h"
#include "gpu/gow3gpu.h"
#include "qte_hook.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <pthread.h>
#include <time.h>
#include <math.h>
#include <SDL3/SDL.h>
#include <sys/stat.h>

#define ERR_INVALID_ARG ((int32_t)0x80920001)
#define ERR_INVALID_HANDLE ((int32_t)0x80920003)
#define ERR_ALREADY_OPENED ((int32_t)0x80920004)
#define ERR_NOT_INITIALIZED ((int32_t)0x80920005)
#define PAD_HANDLE 1

enum {
    BTN_L3=0x2, BTN_R3=0x4, BTN_OPTIONS=0x8, BTN_UP=0x10, BTN_RIGHT=0x20, BTN_DOWN=0x40, BTN_LEFT=0x80,
    BTN_L2=0x100, BTN_R2=0x200, BTN_L1=0x400, BTN_R1=0x800, BTN_TRIANGLE=0x1000, BTN_CIRCLE=0x2000,
    BTN_CROSS=0x4000, BTN_SQUARE=0x8000, BTN_TOUCHPAD=0x100000,
    /* Host-only sources, never shown to the game: a click on one side of the touchpad from a
     * button or key without a touch surface (play_click turns them into a touch and a click). */
    CLICK_LEFT=0x40000000, CLICK_RIGHT=0x80000000,
    /* Host-only actions of the keyboard map: dodge tilts the right stick (apply_dodge); rage and
     * grab press two buttons together (apply_combos). */
    DODGE=0x20000000, RAGE=0x10000000, GRAB=0x08000000,
};
typedef struct { uint16_t x, y; uint8_t id, reserve[3]; } PadTouch;
typedef struct {
    uint32_t buttons;
    uint8_t left_x, left_y, right_x, right_y;
    uint8_t l2, r2, analog_padding[2];
    float orientation[4], acceleration[3], angular_velocity[3];
    uint8_t touch_count, touch_reserve[3];
    uint32_t touch_held_time;
    PadTouch touches[2];
    uint8_t connected, pad0[3];
    uint64_t timestamp;
    uint8_t extension[16];
    uint8_t connected_count, reserve[2], unique_length, unique[12];
} PadData;
typedef struct {
    float pixel_density; uint16_t resolution_x, resolution_y;
    uint8_t dead_zone_left, dead_zone_right, connection_type, connected_count;
    uint8_t connected, pad[3];
    int32_t device_class;
    uint8_t reserve[8];
} ControllerInfo;
_Static_assert(sizeof(PadData)==120,"OrbisPadData layout");
_Static_assert(sizeof(PadTouch)==8,"OrbisPadTouch layout");
_Static_assert(__builtin_offsetof(PadData,touches)==60,"OrbisPadData touch offset");
_Static_assert(__builtin_offsetof(PadData,timestamp)==80,"OrbisPadData timestamp offset");
_Static_assert(sizeof(ControllerInfo)==28,"OrbisPadControllerInformation layout");

static pthread_mutex_t lock=PTHREAD_MUTEX_INITIALIZER;
static int initialized, opened, sdl_ready;
static SDL_Gamepad *gamepad;
static size_t reads;
static uint8_t connected_count;

static uint64_t now_us(void) { struct timespec t; clock_gettime(CLOCK_MONOTONIC,&t); return (uint64_t)t.tv_sec*1000000u+(uint64_t)t.tv_nsec/1000u; }
static uint8_t axis(int16_t v) { int x=(v+32768)>>8; return (uint8_t)(x<0 ? 0 : x>255 ? 255 : x); }
static uint8_t trigger(int16_t v) { int x=v>>7; return (uint8_t)(x<0 ? 0 : x>255 ? 255 : x); }
static uint16_t touch_axis(float v, int max) {
    return (uint16_t)(v<=0.0f ? 0 : v>=1.0f ? max : (int)(v*max+0.5f));
}

/* A click without a touch surface (Back/View, Tab, Backspace, the pad file) is played like a
 * finger: the touch is reported CLICK_LEAD_US before the button, the button stays at least
 * CLICK_HOLD_US, and the touch lingers CLICK_TAIL_US after the release. God of War III opens
 * photo mode on the release of a left click with the finger still down, and ignores a touch
 * that comes and goes with the button. The side is chosen when the click starts and kept: the
 * right source with L1 held is a left click (photo mode on pads without a touchpad). */
enum { CLICK_LEAD_US=50000, CLICK_HOLD_US=50000, CLICK_TAIL_US=100000 };
static struct { int side; uint64_t pressed, released; } click={-1,0,0};
static void play_click(PadData *d) {
    const uint64_t now=d->timestamp;
    uint32_t sources=d->buttons & (CLICK_LEFT|CLICK_RIGHT);
    d->buttons&=~(CLICK_LEFT|CLICK_RIGHT);
    if ((d->buttons & BTN_TOUCHPAD) && !d->touch_count) { /* a replay, or a pad without fingers */
        d->buttons&=~BTN_TOUCHPAD;
        sources|=CLICK_LEFT;
    }
    const int side=(sources & CLICK_LEFT) ? 0 : (sources & CLICK_RIGHT) ? !(d->buttons & BTN_L1) : -1;
    if (side>=0 && (click.side<0 || click.released)) { click.side=side; click.pressed=now; click.released=0; }
    else if (side<0 && click.side>=0 && !click.released) click.released=now;
    if (click.side<0) return;
    const uint64_t down=click.pressed+CLICK_LEAD_US;
    uint64_t up=down+CLICK_HOLD_US;
    if (!click.released) up=UINT64_MAX;
    else if (click.released>up) up=click.released;
    if (now>=up && now-up>=CLICK_TAIL_US) { click.side=-1; return; }
    if (d->touch_count<2) {
        d->touches[d->touch_count]=(PadTouch){.x=click.side ? 1440 : 480,.y=471,.id=d->touch_count};
        ++d->touch_count;
    }
    if (now>=down && now<up) d->buttons|=BTN_TOUCHPAD;
}

/* Opens the first gamepad SDL knows about; called under lock. */
static SDL_Gamepad *current_gamepad(void) {
    if (!sdl_ready) sdl_ready = SDL_WasInit(SDL_INIT_GAMEPAD) ? 1 : SDL_InitSubSystem(SDL_INIT_GAMEPAD) ? 1 : -1;
    if (sdl_ready<0) return NULL;
    if (gamepad && !SDL_GamepadConnected(gamepad)) { SDL_CloseGamepad(gamepad); gamepad=NULL; }
    if (!gamepad) {
        int count=0;
        SDL_JoystickID *ids=SDL_GetGamepads(&count);
        if (ids && count>0) {
            gamepad=SDL_OpenGamepad(ids[0]);
            if (gamepad) { ++connected_count; printf("Runtime: gamepad connected: %s\n",SDL_GetGamepadName(gamepad)); }
        }
        SDL_free(ids);
    }
    return gamepad;
}
/* Button sources: a host button or key and the PS4 button it presses. The defaults below can be
 * changed per PS4 button with GOW3_PAD_MAP (gamepad) and GOW3_KEY_MAP (keyboard), e.g.
 * GOW3_KEY_MAP="cross=Q,square=Left Shift" or GOW3_PAD_MAP="cross=b,circle=a": comma-separated
 * <ps4 button>=<SDL key or gamepad button name>. A mapped PS4 button loses its default sources;
 * <ps4 button>=None leaves it without any.
 * touchpad_left (alias touchpad) and touchpad_right are the two sides of the touchpad click:
 * photo mode and the pause menu. Back/View (Share on a DualSense) is the right click, and with
 * L1 held the left one. Sticks, triggers, the real touchpad click and the overlay shortcuts are
 * not remapped. */
static const struct { const char *name; uint32_t ps; } ps_names[]={
    {"cross",BTN_CROSS}, {"circle",BTN_CIRCLE}, {"square",BTN_SQUARE}, {"triangle",BTN_TRIANGLE},
    {"l1",BTN_L1}, {"r1",BTN_R1}, {"l2",BTN_L2}, {"r2",BTN_R2}, {"l3",BTN_L3}, {"r3",BTN_R3},
    {"options",BTN_OPTIONS}, {"touchpad",CLICK_LEFT}, {"touchpad_left",CLICK_LEFT}, {"touchpad_right",CLICK_RIGHT},
    {"up",BTN_UP}, {"down",BTN_DOWN}, {"left",BTN_LEFT}, {"right",BTN_RIGHT},
    {"dodge",DODGE}, {"rage",RAGE}, {"grab",GRAB},
};
typedef struct { int source; uint32_t ps; } ButtonSource;
typedef struct { ButtonSource entries[48]; size_t count; } ButtonMap;
static ButtonMap pad_map={{
    {SDL_GAMEPAD_BUTTON_SOUTH,BTN_CROSS}, {SDL_GAMEPAD_BUTTON_EAST,BTN_CIRCLE},
    {SDL_GAMEPAD_BUTTON_WEST,BTN_SQUARE}, {SDL_GAMEPAD_BUTTON_NORTH,BTN_TRIANGLE},
    {SDL_GAMEPAD_BUTTON_LEFT_SHOULDER,BTN_L1}, {SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER,BTN_R1},
    {SDL_GAMEPAD_BUTTON_LEFT_STICK,BTN_L3}, {SDL_GAMEPAD_BUTTON_RIGHT_STICK,BTN_R3},
    {SDL_GAMEPAD_BUTTON_START,BTN_OPTIONS}, {SDL_GAMEPAD_BUTTON_BACK,CLICK_RIGHT},
    {SDL_GAMEPAD_BUTTON_TOUCHPAD,BTN_TOUCHPAD},
    {SDL_GAMEPAD_BUTTON_DPAD_UP,BTN_UP}, {SDL_GAMEPAD_BUTTON_DPAD_DOWN,BTN_DOWN},
    {SDL_GAMEPAD_BUTTON_DPAD_LEFT,BTN_LEFT}, {SDL_GAMEPAD_BUTTON_DPAD_RIGHT,BTN_RIGHT},
},15};
static ButtonMap key_map={{
    {SDL_SCANCODE_SPACE,BTN_CROSS}, {SDL_SCANCODE_LSHIFT,BTN_CIRCLE}, {SDL_SCANCODE_E,BTN_SQUARE},
    {SDL_SCANCODE_Q,BTN_TRIANGLE}, {SDL_SCANCODE_1,BTN_L1}, {SDL_SCANCODE_3,BTN_R1},
    {SDL_SCANCODE_R,BTN_L2}, {SDL_SCANCODE_F,BTN_R2}, {SDL_SCANCODE_Z,BTN_L3}, {SDL_SCANCODE_C,BTN_R3},
    {SDL_SCANCODE_RETURN,BTN_OPTIONS}, {SDL_SCANCODE_TAB,CLICK_LEFT}, {SDL_SCANCODE_BACKSPACE,CLICK_RIGHT},
    {SDL_SCANCODE_I,BTN_UP}, {SDL_SCANCODE_K,BTN_DOWN}, {SDL_SCANCODE_J,BTN_LEFT}, {SDL_SCANCODE_L,BTN_RIGHT},
},17};

static char *trim(char *text) {
    while (*text==' ' || *text=='\t') ++text;
    char *end=text+strlen(text);
    while (end>text && (end[-1]==' ' || end[-1]=='\t' || end[-1]=='\r' || end[-1]=='\n')) *--end=0;
    return text;
}

/* Mouse sources of the keyboard map, numbered above every SDL scancode. Buttons are sampled like
 * keys; the wheel has no held state, so each notch becomes a short press (WHEEL_PRESS_US down,
 * then WHEEL_GAP_US up) that the game sees across several reads. */
enum { MOUSE_LEFT=0x10000, MOUSE_RIGHT, MOUSE_MIDDLE, MOUSE_BACK, MOUSE_FORWARD, WHEEL_UP, WHEEL_DOWN };
enum { WHEEL_PRESS_US=60000, WHEEL_GAP_US=40000, WHEEL_MAX_PENDING=4 };
static const struct { const char *name; int source; } mouse_names[]={
    {"Mouse Left",MOUSE_LEFT}, {"Mouse Right",MOUSE_RIGHT}, {"Mouse Middle",MOUSE_MIDDLE},
    {"Mouse Back",MOUSE_BACK}, {"Mouse Forward",MOUSE_FORWARD}, {"Wheel Up",WHEEL_UP}, {"Wheel Down",WHEEL_DOWN},
};
static int mouse_source(const char *name) {
    for (size_t i=0;i<sizeof(mouse_names)/sizeof(*mouse_names);++i)
        if (!SDL_strcasecmp(name,mouse_names[i].name)) return mouse_names[i].source;
    return 0;
}

static SDL_AtomicInt wheel_ticks[2]; /* notches not yet played: 0 up, 1 down */
static struct { int phase; uint64_t until; } wheel_pulse[2]; /* phase 0 idle, 1 pressed, 2 gap */
static SDL_AtomicInt mouse_motion[2]; /* x, y moved since the last read, in 1/16 px (stick prompts) */
/* Runs on the window thread for every queued event (SDL_AddEventWatch). */
static bool SDLCALL mouse_watch(void *userdata, SDL_Event *event) {
    (void)userdata;
    if (event->type==SDL_EVENT_MOUSE_MOTION) {
        SDL_AddAtomicInt(&mouse_motion[0],(int)(event->motion.xrel*16.0f));
        SDL_AddAtomicInt(&mouse_motion[1],(int)(event->motion.yrel*16.0f));
        return true;
    }
    if (event->type!=SDL_EVENT_MOUSE_WHEEL) return true;
    int steps=event->wheel.integer_y;
    if (event->wheel.direction==SDL_MOUSEWHEEL_FLIPPED) steps=-steps;
    SDL_AtomicInt *ticks=&wheel_ticks[steps<0];
    for (int n=steps<0 ? -steps : steps; n>0 && SDL_GetAtomicInt(ticks)<WHEEL_MAX_PENDING; --n) SDL_AddAtomicInt(ticks,1);
    return true;
}
static void watch_wheel(void) {
    static bool watching;
    if (watching || !SDL_WasInit(SDL_INIT_VIDEO)) return;
    watching=SDL_AddEventWatch(mouse_watch,NULL);
}
static void drop_wheel(void) {
    for (int dir=0;dir<2;++dir) {
        SDL_SetAtomicInt(&wheel_ticks[dir],0); wheel_pulse[dir].phase=0;
        SDL_SetAtomicInt(&mouse_motion[dir],0);
    }
}
/* Whether the wheel's direction `dir` is held at `now`; call once per read for each direction. */
static bool wheel_held(int dir, uint64_t now) {
    if (wheel_pulse[dir].phase==1 && now>=wheel_pulse[dir].until) {
        wheel_pulse[dir].phase=2; wheel_pulse[dir].until=now+WHEEL_GAP_US;
    }
    if (wheel_pulse[dir].phase==2 && now>=wheel_pulse[dir].until) wheel_pulse[dir].phase=0;
    if (wheel_pulse[dir].phase==0 && SDL_GetAtomicInt(&wheel_ticks[dir])>0) {
        SDL_AddAtomicInt(&wheel_ticks[dir],-1);
        wheel_pulse[dir].phase=1; wheel_pulse[dir].until=now+WHEEL_PRESS_US;
    }
    return wheel_pulse[dir].phase==1;
}
/* Whether a keyboard-map source (a scancode or a mouse source) is down. */
static bool source_down(int source, const bool *keys, SDL_MouseButtonFlags mouse, const bool wheel[2]) {
    switch (source) {
    case MOUSE_LEFT: return mouse & SDL_BUTTON_LMASK;
    case MOUSE_RIGHT: return mouse & SDL_BUTTON_RMASK;
    case MOUSE_MIDDLE: return mouse & SDL_BUTTON_MMASK;
    case MOUSE_BACK: return mouse & SDL_BUTTON_X1MASK;
    case MOUSE_FORWARD: return mouse & SDL_BUTTON_X2MASK;
    case WHEEL_UP: return wheel[0];
    case WHEEL_DOWN: return wheel[1];
    default: return source>0 && source<SDL_SCANCODE_COUNT && keys[source];
    }
}

/* Scancode of a key name: SDL's names, plus Comma and Equals for the two keys whose own names
 * (',' and '=') are separators of GOW3_KEY_MAP. */
static int key_source(const char *name) {
    if (!SDL_strcasecmp(name,"Comma")) return SDL_SCANCODE_COMMA;
    if (!SDL_strcasecmp(name,"Equals")) return SDL_SCANCODE_EQUALS;
    return (int)SDL_GetScancodeFromName(name);
}

/* Applies a GOW3_PAD_MAP/GOW3_KEY_MAP value; returns how many entries were used. Bad entries are
 * logged and skipped, so their PS4 button keeps its default. */
static int apply_button_map(ButtonMap *map, const char *spec, bool keyboard) {
    if (!spec || !*spec) return 0;
    char copy[1024];
    snprintf(copy,sizeof(copy),"%s",spec);
    int used=0;
    char *entry=copy;
    while (entry) {
        char *next=strchr(entry,',');
        if (next) *next++=0;
        char *eq=strchr(entry,'=');
        if (!eq) {
            if (*trim(entry)) printf("Runtime: pad map: ignored '%s' (expected button=name)\n",trim(entry));
            entry=next;
            continue;
        }
        *eq=0;
        const char *button=trim(entry), *name=trim(eq+1);
        entry=next;
        uint32_t ps=0;
        for (size_t i=0;i<sizeof(ps_names)/sizeof(*ps_names);++i) if (!SDL_strcasecmp(button,ps_names[i].name)) ps=ps_names[i].ps;
        if (ps && !SDL_strcasecmp(name,"None")) { /* the button gets no source at all */
            size_t kept=0;
            for (size_t i=0;i<map->count;++i) if (map->entries[i].ps!=ps) map->entries[kept++]=map->entries[i];
            map->count=kept;
            ++used;
            continue;
        }
        const int mouse=keyboard && *name ? mouse_source(name) : 0;
        const int source=!*name ? -1 : mouse ? mouse : keyboard ? key_source(name)
                                                         : (int)SDL_GetGamepadButtonFromString(name);
        if (!ps || source<=(keyboard ? (int)SDL_SCANCODE_UNKNOWN : (int)SDL_GAMEPAD_BUTTON_INVALID)) {
            printf("Runtime: pad map: ignored '%s=%s'\n",button,name);
            continue;
        }
        size_t kept=0;
        for (size_t i=0;i<map->count;++i) if (map->entries[i].ps!=ps) map->entries[kept++]=map->entries[i];
        map->count=kept;
        if (map->count<sizeof(map->entries)/sizeof(*map->entries)) map->entries[map->count++]=(ButtonSource){source,ps};
        ++used;
    }
    return used;
}

static void load_button_maps(void) {
    static bool loaded;
    if (loaded) return;
    loaded=true;
    const int pad=apply_button_map(&pad_map,getenv("GOW3_PAD_MAP"),false);
    const int keys=apply_button_map(&key_map,getenv("GOW3_KEY_MAP"),true);
    if (pad || keys) printf("Runtime: pad map: %d gamepad and %d keyboard buttons remapped\n",pad,keys);
}

static void key_axis(uint8_t *axis, bool negative, bool positive) {
    if (negative || positive) *axis=(uint8_t)(128-(negative ? 128 : 0)+(positive ? 127 : 0));
}
/* Stick prompts (GOW3_STICK_PROMPTS; the launcher sets it with Native Translation). keyboard: while
 * the game shows a stick-rotation prompt (src/qte_hook.h), W/A/S/D stop moving the left stick and
 * the player types the prompt's keys instead; a finished sequence makes the game's own rotation
 * check pass. A key of the sequence pressed out of turn starts it again; the prompt's timer still
 * decides when the QTE is lost. GOW3_QTE_SEQUENCES replaces sequences, e.g.
 * down180cw=D>S>A,360ccw=A>S>D>W (SDL key names joined by >). mouse: the same sequences drawn with
 * the mouse, each move of the menu's mouse prompt distance in a new direction standing for a key
 * (right D, down S, left A, up W). */
enum { QTE_ACTIVE_US=150000, QTE_FIRST_ROTATION=4, QTE_ROTATIONS=6, QTE_KEYS_MAX=8 };
enum { STICK_PROMPTS_OFF, STICK_PROMPTS_KEYBOARD, STICK_PROMPTS_MOUSE };
static const char *const qte_names[QTE_ROTATIONS]={"up180cw","up180ccw","down180cw","down180ccw","360cw","360ccw"};
/* As the prompts read in play (the game's Down180CW is typed D S A). */
static const char *const qte_defaults[QTE_ROTATIONS]={"A>W>D","D>W>A","D>S>A","A>S>D","D>S>A>W","A>S>D>W"};
static struct { int keys[QTE_KEYS_MAX]; unsigned length; } qte_sequences[QTE_ROTATIONS];
static struct {
    int loaded, mode, rotation;
    uint32_t heartbeat;
    uint64_t seen;       /* the last read that saw the prompt's frame count move */
    unsigned step;
    bool fresh, passed;  /* fresh: keys already held when the prompt appeared do not count */
    bool held[SDL_SCANCODE_COUNT];
    float mouse_x, mouse_y;
    int mouse_key;       /* the direction key the gesture last stood for */
} qte;
static Gow3QteState gow3_qte;
uintptr_t gow3_qte_state_address(void) { return (uintptr_t)&gow3_qte; }

/* "D>S>A" into scancodes: the length, or 0 when a key is unknown or the sequence too long. */
static unsigned qte_parse_sequence(const char *text, int *keys) {
    unsigned n=0;
    char copy[160];
    if (strlen(text)>=sizeof(copy)) return 0;
    strcpy(copy,text);
    for (char *name=copy,*next;name;name=next) {
        next=strchr(name,'>');
        if (next) *next++=0;
        const int key=key_source(trim(name));
        if (n==QTE_KEYS_MAX || key<=SDL_SCANCODE_UNKNOWN || key>=SDL_SCANCODE_COUNT) return 0;
        keys[n++]=key;
    }
    return n;
}

static void qte_load(void) {
    if (qte.loaded) return;
    qte.loaded=1;
    const char *mode=getenv("GOW3_STICK_PROMPTS");
    qte.mode=!mode ? STICK_PROMPTS_OFF : !strcmp(mode,"keyboard") ? STICK_PROMPTS_KEYBOARD
             : !strcmp(mode,"mouse") ? STICK_PROMPTS_MOUSE : STICK_PROMPTS_OFF;
    for (unsigned i=0;i<QTE_ROTATIONS;++i) qte_sequences[i].length=qte_parse_sequence(qte_defaults[i],qte_sequences[i].keys);
    const char *value=getenv("GOW3_QTE_SEQUENCES");
    char entries[512];
    if (value && strlen(value)<sizeof(entries)) {
        strcpy(entries,value);
        for (char *entry=entries,*next;entry;entry=next) {
            next=strchr(entry,',');
            if (next) *next++=0;
            char *keys=strchr(entry,'=');
            unsigned i=0;
            if (keys) {
                *keys++=0;
                while (i<QTE_ROTATIONS && SDL_strcasecmp(trim(entry),qte_names[i])) ++i;
            }
            int parsed[QTE_KEYS_MAX];
            const unsigned n=keys && i<QTE_ROTATIONS ? qte_parse_sequence(keys,parsed) : 0;
            if (n) {
                memcpy(qte_sequences[i].keys,parsed,sizeof(parsed));
                qte_sequences[i].length=n;
            } else if (*trim(entry)) {
                printf("Runtime: stick prompts: ignored '%s'\n",trim(entry));
            }
        }
    }
    if (qte.mode!=STICK_PROMPTS_OFF)
        printf("Runtime: stick prompts: %s%s\n",qte.mode==STICK_PROMPTS_KEYBOARD ? "keyboard" : "adaptive mouse",
               gow3_qte.block ? "" : " unavailable (prompt hooks not installed)");
}

static void qte_reset(void) {
    qte.step=0; qte.passed=false; qte.fresh=true; qte.rotation=-1;
    __atomic_store_n(&gow3_qte.pass,0,__ATOMIC_RELAXED);
}

/* The rotation (0..5) the prompt on screen asks for, else -1. A prompt is up while its update
 * keeps counting frames; QTE_ACTIVE_US without a frame ends it (and resets the minigame). */
static int qte_rotation(uint64_t now) {
    if (!gow3_qte.block) return -1;
    const uint32_t beat=__atomic_load_n(&gow3_qte.heartbeat,__ATOMIC_RELAXED);
    if (beat!=qte.heartbeat) { qte.heartbeat=beat; qte.seen=now; }
    if (now-qte.seen>QTE_ACTIVE_US) { qte_reset(); return -1; }
    const unsigned type=*(const volatile uint16_t *)(uintptr_t)(gow3_qte.block+GOW3_QTE_TYPE_OFFSET);
    return type-QTE_FIRST_ROTATION<QTE_ROTATIONS ? (int)(type-QTE_FIRST_ROTATION) : -1;
}

/* A direction key (pressed or drawn) in the minigame: the next step, or back to the start. */
static void qte_advance(int key, const int *sequence, unsigned length) {
    if (qte.passed) return;
    qte.step=key==sequence[qte.step] ? qte.step+1 : key==sequence[0];
    if (qte.step==length) {
        qte.passed=true;
        __atomic_store_n(&gow3_qte.pass,1,__ATOMIC_RELEASE);
    }
}

/* The key the mouse gesture stands for (right D, down S, left A, up W) once it has moved the mouse
 * prompt distance since its last step and turned a new way, else 0. fresh: forget older motion. */
static int qte_mouse_key(bool fresh) {
    const float dx=SDL_SetAtomicInt(&mouse_motion[0],0)/16.0f, dy=SDL_SetAtomicInt(&mouse_motion[1],0)/16.0f;
    if (fresh) { qte.mouse_x=qte.mouse_y=0; qte.mouse_key=0; return 0; }
    qte.mouse_x+=dx; qte.mouse_y+=dy;
    const float step=(float)gow3gpu_mouse_prompt_distance();
    if (qte.mouse_x*qte.mouse_x+qte.mouse_y*qte.mouse_y<step*step) return 0;
    const float ax=qte.mouse_x<0 ? -qte.mouse_x : qte.mouse_x, ay=qte.mouse_y<0 ? -qte.mouse_y : qte.mouse_y;
    const int key=ax>=ay ? (qte.mouse_x>0 ? SDL_SCANCODE_D : SDL_SCANCODE_A)
                         : (qte.mouse_y>0 ? SDL_SCANCODE_S : SDL_SCANCODE_W);
    qte.mouse_x=qte.mouse_y=0;
    if (key==qte.mouse_key) return 0;
    qte.mouse_key=key;
    return key;
}

/* One read of the stick prompts (keys: SDL's keyboard state); true while they own W/A/S/D. */
static bool qte_play(const bool *keys, uint64_t now) {
    qte_load();
    if (qte.mode==STICK_PROMPTS_OFF) return false;
    const int rotation=qte_rotation(now);
    if (rotation<0) return false;
    if (rotation!=qte.rotation) { qte.rotation=rotation; qte.step=0; }
    const int *sequence=qte_sequences[rotation].keys;
    const unsigned length=qte_sequences[rotation].length;
    if (qte.mode==STICK_PROMPTS_MOUSE) {
        const int key=qte_mouse_key(qte.fresh);
        if (key) qte_advance(key,sequence,length);
    } else {
        for (unsigned i=0;i<length;++i) {
            const int key=sequence[i];
            unsigned first=0;
            while (sequence[first]!=key) ++first;
            if (first<i) continue;  /* a key listed twice is looked at once */
            const bool pressed=keys[key] && !qte.held[key] && !qte.fresh;
            qte.held[key]=keys[key];
            if (pressed) qte_advance(key,sequence,length);
        }
    }
    qte.fresh=false;
    return true;
}

static void sample_host(PadData *d) {
    memset(d,0,sizeof(*d));
    d->left_x=d->left_y=d->right_x=d->right_y=128;
    d->orientation[3]=1.0f;
    d->connected=1; d->connected_count=connected_count ? connected_count : 1;
    d->timestamp=now_us();
    SDL_Gamepad *g=current_gamepad();
    watch_wheel();
    if (gow3gpu_overlay_captures_input()) { drop_wheel(); return; } /* settings menu open: neutral input */
    const bool *k=SDL_WasInit(SDL_INIT_VIDEO) ? SDL_GetKeyboardState(NULL) : NULL;
    load_button_maps();
    if (g) {
        for (size_t i=0;i<pad_map.count;++i)
            if (SDL_GetGamepadButton(g,(SDL_GamepadButton)pad_map.entries[i].source)) d->buttons|=pad_map.entries[i].ps;
        const uint32_t mapped=d->buttons;
        d->left_x=axis(SDL_GetGamepadAxis(g,SDL_GAMEPAD_AXIS_LEFTX)); d->left_y=axis(SDL_GetGamepadAxis(g,SDL_GAMEPAD_AXIS_LEFTY));
        d->right_x=axis(SDL_GetGamepadAxis(g,SDL_GAMEPAD_AXIS_RIGHTX)); d->right_y=axis(SDL_GetGamepadAxis(g,SDL_GAMEPAD_AXIS_RIGHTY));
        d->l2=trigger(SDL_GetGamepadAxis(g,SDL_GAMEPAD_AXIS_LEFT_TRIGGER)); d->r2=trigger(SDL_GetGamepadAxis(g,SDL_GAMEPAD_AXIS_RIGHT_TRIGGER));
        if (d->l2>30) d->buttons|=BTN_L2;
        if (d->r2>30) d->buttons|=BTN_R2;
        if (mapped & BTN_L2) d->l2=255;
        if (mapped & BTN_R2) d->r2=255;
        if (SDL_GetNumGamepadTouchpads(g)>0) {
            const int fingers=SDL_GetNumGamepadTouchpadFingers(g,0);
            for (int finger=0;finger<fingers && d->touch_count<2;++finger) {
                bool down=false;
                float x=0, y=0;
                if (SDL_GetGamepadTouchpadFinger(g,0,finger,&down,&x,&y,NULL) && down) {
                    d->touches[d->touch_count++]=(PadTouch){.x=touch_axis(x,1919),
                        .y=touch_axis(y,942),.id=(uint8_t)finger};
                }
            }
        }
    }
    /* The keyboard works with or without a gamepad: its buttons add to the pad's, and a stick
     * follows the keys only while one of them is held. */
    if (!k) return;
    const SDL_MouseButtonFlags mouse=SDL_GetMouseState(NULL,NULL);
    const bool wheel[2]={wheel_held(0,d->timestamp),wheel_held(1,d->timestamp)};
    for (size_t i=0;i<key_map.count;++i)
        if (source_down(key_map.entries[i].source,k,mouse,wheel)) d->buttons|=key_map.entries[i].ps;
    if (d->buttons & BTN_L2) d->l2=255;
    if (d->buttons & BTN_R2) d->r2=255;
    if (!qte_play(k,d->timestamp)) {
        key_axis(&d->left_x,k[SDL_SCANCODE_A],k[SDL_SCANCODE_D]);
        key_axis(&d->left_y,k[SDL_SCANCODE_W],k[SDL_SCANCODE_S]);
    }
    key_axis(&d->right_x,k[SDL_SCANCODE_LEFT],k[SDL_SCANCODE_RIGHT]);
    key_axis(&d->right_y,k[SDL_SCANCODE_UP],k[SDL_SCANCODE_DOWN]);
}

/* GOW3_PAD_FILE=<file>: scripted input for automated runs. The file holds whitespace-separated
 * tokens, re-read when it changes: button names (cross circle square triangle l1 r1 l2 r2 l3 r3
 * options touchpad touchpad_left touchpad_right up down left right) are held while listed;
 * touchpad is the left side; lx= ly= rx= ry= (0..255) override the sticks. An empty file
 * releases everything. */
static struct { uint32_t buttons; int stick[4]; } injected={0,{-1,-1,-1,-1}};
static int replay_armed;      /* 1 while a GOW3_PAD_REPLAY recording plays, 2 once it ended */
static uint64_t replay_start; /* 0: (re)start at the next sample */
static void read_inject(void) {
    static const char *path; static int checked; static uint64_t last_check; static struct timespec mtime;
    if (!checked) { path=getenv("GOW3_PAD_FILE"); checked=1; }
    if (!path || !*path) return;
    uint64_t now=now_us();
    if (now-last_check<20000) return;
    last_check=now;
    struct stat st;
    if (stat(path,&st)!=0) return;
#ifdef _WIN32
    /* Whole-second mtimes: the size tells two edits within a second apart. */
    if (st.st_mtime==mtime.tv_sec && st.st_size==mtime.tv_nsec) return;
    mtime=(struct timespec){st.st_mtime,(long)st.st_size};
#else
    if (st.st_mtim.tv_sec==mtime.tv_sec && st.st_mtim.tv_nsec==mtime.tv_nsec) return;
    mtime=st.st_mtim;
#endif
    FILE *f=fopen(path,"r");
    if (!f) return;
    static const char *sticks[]={"lx=","ly=","rx=","ry="};
    injected.buttons=0;
    for (int i=0;i<4;++i) injected.stick[i]=-1;
    char token[64];
    while (fscanf(f,"%63s",token)==1) {
        if (!strcmp(token,"replay") && replay_armed!=1) { replay_armed=1; replay_start=0; } /* GOW3_PAD_REPLAY */
        for (size_t i=0;i<sizeof(ps_names)/sizeof(*ps_names);++i) if (!strcmp(token,ps_names[i].name)) injected.buttons|=ps_names[i].ps;
        for (int i=0;i<4;++i) if (!strncmp(token,sticks[i],3)) { int v=atoi(token+3); injected.stick[i]=v<0 ? 0 : v>255 ? 255 : v; }
    }
    fclose(f);
    printf("Runtime: pad file: buttons 0x%x sticks %d %d %d %d\n",injected.buttons,
           injected.stick[0],injected.stick[1],injected.stick[2],injected.stick[3]);
}
/* GOW3_PAD_RECORD=<file>: F9 starts and stops recording the pad state (gamepad or keyboard) with
 * the time since F9; GOW3_PAD_REPLAY=<file> plays such a recording back, started by the token
 * "replay" in GOW3_PAD_FILE (scripted tests repeat a route the player ran once). Lines: ms buttons
 * lx ly rx ry l2 r2, written when the state changes. */
typedef struct { uint32_t ms, buttons; uint8_t axes[4], l2, r2; } PadSample;
static FILE *record_file;
static uint64_t record_start;
static PadSample record_last;
static void record_sample(const PadData *d) {
    static const char *path; static int checked, f9_was_down;
    if (!checked) { path=getenv("GOW3_PAD_RECORD"); checked=1; }
    if (!path || !*path || !sdl_ready) return;
    const bool *k=SDL_GetKeyboardState(NULL);
    const int f9=k && k[SDL_SCANCODE_F9];
    if (f9 && !f9_was_down) {
        if (record_file) {
            fclose(record_file); record_file=NULL;
            printf("Runtime: pad recording stopped (%s)\n",path);
        } else if ((record_file=fopen(path,"w"))) {
            record_start=now_us();
            memset(&record_last,0xff,sizeof(record_last));
            printf("Runtime: pad recording started (%s, F9 stops)\n",path);
        }
    }
    f9_was_down=f9;
    if (!record_file) return;
    PadSample s={(uint32_t)((now_us()-record_start)/1000),d->buttons,
                 {d->left_x,d->left_y,d->right_x,d->right_y},d->l2,d->r2};
    if (s.buttons==record_last.buttons && !memcmp(s.axes,record_last.axes,4) &&
        s.l2==record_last.l2 && s.r2==record_last.r2) return;
    record_last=s;
    fprintf(record_file,"%u %u %u %u %u %u %u %u\n",s.ms,s.buttons,s.axes[0],s.axes[1],s.axes[2],
            s.axes[3],s.l2,s.r2);
    fflush(record_file);
}
static PadSample *replay; static size_t replay_count, replay_next;
static void replay_sample(PadData *d) {
    if (!replay_armed) return;
    if (!replay_start) {
        static int loaded;
        if (!loaded) {
            loaded=1;
            const char *path=getenv("GOW3_PAD_REPLAY");
            FILE *f=path ? fopen(path,"r") : NULL;
            PadSample s; unsigned v[8]; size_t cap=0;
            while (f && fscanf(f,"%u %u %u %u %u %u %u %u",&v[0],&v[1],&v[2],&v[3],&v[4],&v[5],&v[6],&v[7])==8) {
                s=(PadSample){v[0],v[1],{(uint8_t)v[2],(uint8_t)v[3],(uint8_t)v[4],(uint8_t)v[5]},(uint8_t)v[6],(uint8_t)v[7]};
                if (replay_count==cap && !(replay=realloc(replay,(cap=cap ? cap*2 : 1024)*sizeof(*replay)))) break;
                replay[replay_count++]=s;
            }
            if (f) fclose(f);
            printf("Runtime: pad replay of %zu samples from %s\n",replay_count,path ? path : "(unset)");
        }
        replay_start=now_us();
        replay_next=0;
    }
    const uint32_t ms=(uint32_t)((now_us()-replay_start)/1000);
    while (replay_next<replay_count && replay[replay_next].ms<=ms) ++replay_next;
    if (!replay_next) return;
    if (replay_next==replay_count && ms>replay[replay_count-1].ms+500) {
        if (replay_armed==1) { puts("Runtime: pad replay finished"); replay_armed=2; }
        return;
    }
    const PadSample *s=&replay[replay_next-1];
    d->buttons=s->buttons;
    d->left_x=s->axes[0]; d->left_y=s->axes[1]; d->right_x=s->axes[2]; d->right_y=s->axes[3];
    d->l2=s->l2; d->r2=s->r2;
}
/* L2 aim (the items, e.g. the Head of Helios' light), in the menu's right stick style.
 * Relative: while L2 is held, moving the mouse pushes the right stick the way it moves, as far as
 * it moves fast (full tilt at AIM_FULL_PX_PER_MS); when the mouse stops the stick is left alone
 * again. A direction holds AIM_HOLD_US after the last movement, over the gaps between mouse reports.
 * Absolute: where the mouse was when L2 was pressed is the centre, and the stick points from it to
 * where the mouse is (full tilt AIM_RADIUS_PX away at 100 %; the offset stops there, so coming back
 * works at once). The menu's sensitivity divides both distances. Reads after the stick prompts, so a
 * prompt drawn with the mouse comes first. */
enum { AIM_HOLD_US=40000, AIM_STEP_MIN_US=1000, AIM_STEP_MAX_US=50000, AIM_RADIUS_PX=200 };
static const float AIM_FULL_PX_PER_MS=1.2f;
static struct { uint64_t last, moved; int x, y; bool held; float offset_x, offset_y; } aim;
static void apply_mouse_aim(PadData *d) {
    const float dx=SDL_SetAtomicInt(&mouse_motion[0],0)/16.0f, dy=SDL_SetAtomicInt(&mouse_motion[1],0)/16.0f;
    uint64_t step=d->timestamp-aim.last;
    aim.last=d->timestamp;
    if (!(d->buttons & BTN_L2)) { aim.moved=0; aim.held=false; return; }
    if (!aim.held) { aim.held=true; aim.offset_x=aim.offset_y=0; }  /* a new press: the centre is here */
    const float sensitivity=gow3gpu_mouse_aim_sensitivity()/100.0f;
    if (gow3gpu_mouse_aim_style()==1) {
        const float radius=AIM_RADIUS_PX/sensitivity;
        aim.offset_x+=dx; aim.offset_y+=dy;
        const float length=sqrtf(aim.offset_x*aim.offset_x+aim.offset_y*aim.offset_y);
        if (length>radius) { aim.offset_x*=radius/length; aim.offset_y*=radius/length; }
        if (aim.offset_x!=0 || aim.offset_y!=0) {
            d->right_x=(uint8_t)(128+(int)(aim.offset_x*127/radius));
            d->right_y=(uint8_t)(128+(int)(aim.offset_y*127/radius));
        }
        return;
    }
    if (dx!=0 || dy!=0) {
        step=step<AIM_STEP_MIN_US ? AIM_STEP_MIN_US : step>AIM_STEP_MAX_US ? AIM_STEP_MAX_US : step;
        const float scale=127.0f*1000.0f*sensitivity/(AIM_FULL_PX_PER_MS*(float)step);
        const float x=dx*scale, y=dy*scale;
        aim.x=x>127 ? 127 : x<-128 ? -128 : (int)x;
        aim.y=y>127 ? 127 : y<-128 ? -128 : (int)y;
        aim.moved=d->timestamp;
    }
    if (aim.moved && d->timestamp-aim.moved<AIM_HOLD_US) {
        d->right_x=(uint8_t)(128+aim.x);
        d->right_y=(uint8_t)(128+aim.y);
    }
}

/* The pause key (touchpad right without L1, which is photo mode) lets go of the window's mouse
 * lock while the game's pause menu is up, and takes it back (gpu/shim/window.cpp). */
static void note_pause_key(const PadData *d) {
    static bool held;
    const bool pause=(d->buttons & CLICK_RIGHT) && !(d->buttons & BTN_L1);
    if (pause && !held) gow3gpu_mouse_pause_key();
    held=pause;
}

/* One source for two buttons pressed in the same read (GOW3_KEY_MAP "rage=<key>", "grab=<key>"):
 * L3 + R3 for Rage of Sparta, L1 + Circle for the grab. */
static void apply_combos(PadData *d) {
    static const struct { uint32_t action, buttons; } combos[]={{RAGE,BTN_L3|BTN_R3},{GRAB,BTN_L1|BTN_CIRCLE}};
    for (size_t i=0;i<sizeof(combos)/sizeof(*combos);++i)
        if (d->buttons & combos[i].action) d->buttons=(d->buttons & ~combos[i].action) | combos[i].buttons;
}

/* Dodge (GOW3_KEY_MAP "dodge=<key>"): God of War III evades toward where the right stick is pushed.
 * While the dodge source is held the right stick points where the left stick does (WASD), at full
 * tilt, or straight back (toward the camera) when standing still; a tap holds it DODGE_MIN_US so
 * the game sees it. */
enum { DODGE_MIN_US=80000, DODGE_DEADZONE=40 };
static struct { bool held; uint64_t until; uint8_t x, y; } dodge={false,0,128,255};
static void reset_dodge(void) { dodge.held=false; dodge.until=0; }
static void apply_dodge(PadData *d) {
    const bool held=(d->buttons & DODGE)!=0;
    d->buttons&=~DODGE;
    if (held) {
        int dx=d->left_x-128, dy=d->left_y-128;
        if (abs(dx)<DODGE_DEADZONE && abs(dy)<DODGE_DEADZONE) { dx=0; dy=127; }
        const int reach=abs(dx)>abs(dy) ? abs(dx) : abs(dy);
        const int x=128+dx*128/reach, y=128+dy*128/reach;
        dodge.x=(uint8_t)(x<0 ? 0 : x>255 ? 255 : x);
        dodge.y=(uint8_t)(y<0 ? 0 : y>255 ? 255 : y);
        if (!dodge.held) dodge.until=d->timestamp+DODGE_MIN_US;
    }
    dodge.held=held;
    if (held || d->timestamp<dodge.until) { d->right_x=dodge.x; d->right_y=dodge.y; }
}

/* After the menu or the text dialog closes, buttons still held (the Cross that accepted a
 * name) stay hidden until released: the game would take them as a new press. */
static int hold_after_capture;
static void sample(PadData *d) {
    sample_host(d);
    if (gow3gpu_overlay_captures_input()) { hold_after_capture=1; click.side=-1; reset_dodge(); return; }
    record_sample(d); /* the click sources as they are: a replay plays them again */
    read_inject();
    replay_sample(d);
    d->buttons|=injected.buttons;
    if (injected.buttons & BTN_L2) d->l2=255;
    if (injected.buttons & BTN_R2) d->r2=255;
    uint8_t *axes[4]={&d->left_x,&d->left_y,&d->right_x,&d->right_y};
    for (int i=0;i<4;++i) if (injected.stick[i]>=0) *axes[i]=(uint8_t)injected.stick[i];
    apply_combos(d);
    apply_dodge(d);
    apply_mouse_aim(d);
    note_pause_key(d);
    play_click(d);
    if (hold_after_capture) {
        if (d->buttons) d->buttons=0;
        else hold_after_capture=0;
    }
}

static ABI int32_t pad_init(void) { pthread_mutex_lock(&lock); initialized=1; pthread_mutex_unlock(&lock); return 0; }
static ABI int32_t pad_open(int32_t user, int32_t type, int32_t index, const void *param) {
    (void)param;
    if (!initialized) return ERR_NOT_INITIALIZED;
    if (user!=1) return ERR_INVALID_ARG;
    if (type!=0 && type!=2) return ERR_INVALID_ARG; /* standard / special port */
    if (index) return ERR_INVALID_ARG;
    pthread_mutex_lock(&lock);
    int already=opened; opened=1;
    pthread_mutex_unlock(&lock);
    if (already) return ERR_ALREADY_OPENED;
    puts("Runtime: pad opened for user 1 (SDL gamepad or keyboard)");
    return PAD_HANDLE;
}
static ABI int32_t pad_close(int32_t handle) {
    if (handle!=PAD_HANDLE || !opened) return ERR_INVALID_HANDLE;
    opened=0; return 0;
}
static ABI int32_t pad_read_state(int32_t handle, PadData *data) {
    if (handle!=PAD_HANDLE || !opened) return ERR_INVALID_HANDLE;
    if (!data) return ERR_INVALID_ARG;
    pthread_mutex_lock(&lock);
    sample(data); ++reads;
    pthread_mutex_unlock(&lock);
    return 0;
}
/* Buffered read: the port samples once per call, so one entry is returned. */
static ABI int32_t pad_read(int32_t handle, PadData *data, int32_t count) {
    if (handle!=PAD_HANDLE || !opened) return ERR_INVALID_HANDLE;
    if (!data || count<1 || count>64) return ERR_INVALID_ARG;
    pad_read_state(handle,data);
    return 1;
}
static ABI int32_t pad_info(int32_t handle, ControllerInfo *info) {
    if (handle!=PAD_HANDLE || !opened) return ERR_INVALID_HANDLE;
    if (!info) return ERR_INVALID_ARG;
    memset(info,0,sizeof(*info));
    info->pixel_density=44.86f; info->resolution_x=1920; info->resolution_y=943;
    info->dead_zone_left=info->dead_zone_right=2;
    info->connection_type=0; info->connected=1; info->device_class=0;
    pthread_mutex_lock(&lock);
    current_gamepad();
    info->connected_count=connected_count ? connected_count : 1;
    pthread_mutex_unlock(&lock);
    return 0;
}
static ABI int32_t pad_vibration(int32_t handle, const uint8_t *param) {
    if (handle!=PAD_HANDLE || !opened) return ERR_INVALID_HANDLE;
    if (!param) return ERR_INVALID_ARG;
    pthread_mutex_lock(&lock);
    SDL_Gamepad *g=current_gamepad();
    /* The menu's vibration strength scales the game's motors (0: off). */
    const unsigned percent=(unsigned)gow3gpu_vibration_percent();
    if (g) SDL_RumbleGamepad(g,(uint16_t)(param[0]*257u*percent/100u),(uint16_t)(param[1]*257u*percent/100u),1000);
    pthread_mutex_unlock(&lock);
    return 0;
}
static ABI int32_t pad_ok_handle(int32_t handle) { return handle==PAD_HANDLE && opened ? 0 : ERR_INVALID_HANDLE; }
static ABI int32_t pad_ok_handle_flag(int32_t handle, uint8_t flag) { (void)flag; return pad_ok_handle(handle); }

static ABI int32_t pad_light_bar(int32_t handle, const uint8_t *rgb) {
    if (handle!=PAD_HANDLE || !opened) return ERR_INVALID_HANDLE;
    (void)rgb; return 0;
}
static const RuntimeExport exports[]={
    {"scePadSetLightBar",pad_light_bar}, {"scePadResetLightBar",pad_ok_handle},
    {"scePadInit",pad_init}, {"scePadOpen",pad_open}, {"scePadClose",pad_close},
    {"scePadReadState",pad_read_state}, {"scePadRead",pad_read},
    {"scePadGetControllerInformation",pad_info}, {"scePadSetVibration",pad_vibration},
    {"scePadResetOrientation",pad_ok_handle},
    {"scePadSetAngularVelocityDeadbandState",pad_ok_handle_flag}, {"scePadSetTiltCorrectionState",pad_ok_handle_flag},
    {"scePadSetMotionSensorState",pad_ok_handle_flag},
};
uintptr_t runtime_pad_resolve(const char *name) { return RUNTIME_LOOKUP(exports,name); }
void runtime_pad_report(void) { printf("Runtime: pad reads=%zu, gamepad=%s\n",reads,gamepad ? SDL_GetGamepadName(gamepad) : "none"); }

/* probe --gamepad: runs before any game state exists, so it uses its own SDL gamepad and leaves
 * the lock and statics above alone. run.py parses this line; keep the format. */
static const char *label_name(SDL_GamepadButtonLabel label) {
    switch (label) {
    case SDL_GAMEPAD_BUTTON_LABEL_A: return "a";
    case SDL_GAMEPAD_BUTTON_LABEL_B: return "b";
    case SDL_GAMEPAD_BUTTON_LABEL_X: return "x";
    case SDL_GAMEPAD_BUTTON_LABEL_Y: return "y";
    case SDL_GAMEPAD_BUTTON_LABEL_CROSS: return "cross";
    case SDL_GAMEPAD_BUTTON_LABEL_CIRCLE: return "circle";
    case SDL_GAMEPAD_BUTTON_LABEL_SQUARE: return "square";
    case SDL_GAMEPAD_BUTTON_LABEL_TRIANGLE: return "triangle";
    default: return "unknown";
    }
}
int runtime_pad_describe(void) {
    SDL_Gamepad *g=NULL;
    const bool ready=SDL_InitSubSystem(SDL_INIT_GAMEPAD);
    /* No event loop yet, so pads show up only after a pump: poll for about 300 ms. */
    for (int waited=0; ready && !g && waited<30; ++waited) {
        SDL_PumpEvents();
        int count=0;
        SDL_JoystickID *ids=SDL_GetGamepads(&count);
        if (ids && count>0) g=SDL_OpenGamepad(ids[0]);
        SDL_free(ids);
        if (!g) SDL_Delay(10);
    }
    if (!g) puts("Gamepad: none");
    else {
        const char *type=SDL_GetGamepadStringForType(SDL_GetGamepadType(g));
        const char *name=SDL_GetGamepadName(g);
        printf("Gamepad: type=%s south=%s east=%s west=%s north=%s name=%s\n",
               type ? type : "unknown",
               label_name(SDL_GetGamepadButtonLabel(g,SDL_GAMEPAD_BUTTON_SOUTH)),
               label_name(SDL_GetGamepadButtonLabel(g,SDL_GAMEPAD_BUTTON_EAST)),
               label_name(SDL_GetGamepadButtonLabel(g,SDL_GAMEPAD_BUTTON_WEST)),
               label_name(SDL_GetGamepadButtonLabel(g,SDL_GAMEPAD_BUTTON_NORTH)),
               name ? name : "unknown");
        SDL_CloseGamepad(g);
    }
    if (ready) SDL_QuitSubSystem(SDL_INIT_GAMEPAD);
    return g!=NULL;
}
