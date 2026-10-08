/* Narrow, explicit PS4 libc contracts. No automatic success stubs. */
#define _CRT_RAND_S
#include "runtime.h"
#include "gpu/bbgpu.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdatomic.h>
#include <sched.h>
#include <pthread.h>

typedef struct {
    union { GuestCallback plain; void (ABI *with_arg)(void *); } callback;
    void *argument, *dso;
    int with_arg, active;
} ExitHandler;
static ExitHandler *handlers;
static size_t handler_count, handler_capacity, calls_init, calls_atexit, calls_cxa;
static uint64_t capabilities;
static _Atomic size_t guards_acquired, guards_released;
static uint64_t stack_canary;
static _Atomic size_t memory_calls;
/* Dynamic TLS of linked modules: module IDs 2..7 (1 is the eboot's static TLS). */
#define TLS_MODULES 8
static struct { const void *data; uint64_t filesz, memsz; } tls_modules[TLS_MODULES];
static _Thread_local unsigned char *tls_blocks[TLS_MODULES];
static void *process_param;
void runtime_set_procparam(void *param) { process_param=param; }
static ABI void *guest_procparam(void) { return process_param; }
static void **application_heap_api;
static ABI void guest_set_heap_api(void **api) {
    application_heap_api=api;
    puts("Runtime: application heap API registered");
}
void **runtime_application_heap_api(void) { return application_heap_api; }
void runtime_set_module_tls(uint64_t module, const void *data, uint64_t filesz, uint64_t memsz) {
    if (module<2 || module>=TLS_MODULES) { fputs("STOP: unsupported TLS module id\n",stderr); exit(21); }
    tls_modules[module].data=data; tls_modules[module].filesz=filesz; tls_modules[module].memsz=memsz;
}
void runtime_set_libc_tls(const void *data, uint64_t filesz, uint64_t memsz) { runtime_set_module_tls(2,data,filesz,memsz); }
static ABI void *guest_tls_get_addr(const uint64_t *index) {
    if (!index || index[0]<2 || index[0]>=TLS_MODULES || !tls_modules[index[0]].data || index[1]>=tls_modules[index[0]].memsz) {
        fputs("STOP: unsupported TLS module/offset\n",stderr); exit(21);
    }
    unsigned char **block=&tls_blocks[index[0]];
    if (!*block) {
        *block=calloc(1,tls_modules[index[0]].memsz);
        if (!*block) { fputs("Cannot allocate module TLS\n",stderr); exit(1); }
        memcpy(*block,tls_modules[index[0]].data,tls_modules[index[0]].filesz);
    }
    return *block+index[1];
}

void runtime_start(uint64_t flags) {
    capabilities = flags;
    if (!(flags & 1)) return;
#ifdef _WIN32
    unsigned int halves[2];
    if (rand_s(&halves[0]) || rand_s(&halves[1])) { fputs("Cannot initialize stack canary\n", stderr); exit(1); }
    memcpy(&stack_canary, halves, sizeof(stack_canary));
#else
    FILE *random = fopen("/dev/urandom", "rb");
    if (!random || fread(&stack_canary, sizeof(stack_canary), 1, random) != 1) {
        fputs("Cannot initialize stack canary\n", stderr); exit(1);
    }
    fclose(random);
#endif
    stack_canary &= ~UINT64_C(255);
}
void runtime_report(void) {
    printf("Runtime: _init_env=%zu, atexit=%zu, __cxa_atexit=%zu, registered handlers=%zu\n",
           calls_init, calls_atexit, calls_cxa, handler_count);
    printf("Runtime: static guards acquired=%zu, released=%zu\n", (size_t)guards_acquired, (size_t)guards_released);
    runtime_thread_report();
    runtime_mutex_report();
    runtime_rwlock_report();
    runtime_sema_report();
    runtime_content_report();
    runtime_memory_report();
    runtime_ajm_report();
    runtime_audio_report();
    runtime_pad_report();
    runtime_savedata_report();
    runtime_file_report();
    printf("Runtime: memory operations=%zu\n", (size_t)memory_calls);
}
static ABI void init_env(void) {
    /* prepare.py verified the supplied libc export consists of exactly C3. */
    ++calls_init;
    puts("Runtime: _init_env returned (verified libc implementation: RET)");
}
static pthread_mutex_t handler_lock = PTHREAD_MUTEX_INITIALIZER;
static int register_handler(ExitHandler value) {
    pthread_mutex_lock(&handler_lock);
    if (handler_count == handler_capacity) {
        size_t capacity = handler_capacity ? handler_capacity * 2 : 64;
        ExitHandler *next = capacity > 1024 * 1024 ? NULL : realloc(handlers, capacity * sizeof(*handlers));
        if (!next) { pthread_mutex_unlock(&handler_lock); return -1; }
        handlers = next; handler_capacity = capacity;
    }
    value.active = 1; handlers[handler_count++] = value;
    pthread_mutex_unlock(&handler_lock);
    return 0;
}
static ABI int guest_atexit(GuestCallback callback) {
    if (!callback) return -1;
    ++calls_atexit;
    return register_handler((ExitHandler){.callback.plain=callback});
}
static ABI int guest_cxa_atexit(void (ABI *callback)(void *), void *argument, void *dso) {
    if (!callback) return -1;
    ++calls_cxa;
    return register_handler((ExitHandler){.callback.with_arg=callback, .argument=argument, .dso=dso, .with_arg=1});
}
void runtime_finalize(void *dso) {
    /* Mark before invoking: repeated or recursive finalization cannot run twice.
       Restart at the end to include handlers registered by a destructor. */
    for (;;) {
        pthread_mutex_lock(&handler_lock);
        size_t i = handler_count;
        while (i && (!handlers[i-1].active || (dso && handlers[i-1].dso != dso))) --i;
        if (!i) { pthread_mutex_unlock(&handler_lock); return; }
        ExitHandler handler = handlers[i-1]; handlers[i-1].active = 0;
        pthread_mutex_unlock(&handler_lock);
        if (handler.with_arg) handler.callback.with_arg(handler.argument);
        else handler.callback.plain();
    }
}
static ABI void guest_finalize(void *dso) { runtime_finalize(dso); }
static _Atomic uint32_t next_guard_owner=1;
static _Thread_local uint32_t guard_owner;
static ABI int guard_acquire(uint64_t *guard) {
    _Atomic uint64_t *state = (_Atomic uint64_t *)guard;
    /* Dump libc uses the low byte for completion and the dword at +4 as lock.
       The lock word holds a per-thread owner tag so recursion is detectable. */
    if (!guard_owner) guard_owner = atomic_fetch_add(&next_guard_owner, 1);
    for (;;) {
        uint64_t expected = 0;
        if (atomic_load_explicit(state, memory_order_acquire) & 1) return 0;
        if (atomic_compare_exchange_strong_explicit(state, &expected, (uint64_t)guard_owner << 32,
                                                    memory_order_acq_rel, memory_order_acquire)) break;
        if (expected & 1) return 0;
        if ((uint32_t)(expected >> 32) == guard_owner) {
            fputs("STOP: recursive/concurrent static initialization is not supported yet\n", stderr);
            exit(21);
        }
        sched_yield(); /* another thread is running the initializer */
    }
    atomic_fetch_add(&guards_acquired, 1);
    return 1;
}
static ABI void guard_release(uint64_t *guard) {
    atomic_store_explicit((_Atomic uint64_t *)guard, 1, memory_order_release);
    atomic_fetch_add(&guards_released, 1);
}
static ABI void guard_abort(uint64_t *guard) {
    atomic_fetch_and_explicit((_Atomic uint64_t *)guard, UINT64_C(0xffffffff), memory_order_release);
}
static ABI __attribute__((noreturn)) void stack_fail(void) {
    fputs("STOP: guest stack protector detected corruption\n", stderr);
    exit(22);
}
static ABI void *guest_memset(void *dst, int value, size_t size) {
    atomic_fetch_add_explicit(&memory_calls,1,memory_order_relaxed); return memset(dst, value, size);
}
static ABI void *guest_memcpy(void *dst, const void *src, size_t size) {
    atomic_fetch_add_explicit(&memory_calls,1,memory_order_relaxed); return memcpy(dst, src, size);
}
static ABI void *guest_memmove(void *dst, const void *src, size_t size) {
    atomic_fetch_add_explicit(&memory_calls,1,memory_order_relaxed); return memmove(dst, src, size);
}
static ABI int guest_memcmp(const void *a, const void *b, size_t size) {
    atomic_fetch_add_explicit(&memory_calls,1,memory_order_relaxed); return memcmp(a, b, size);
}
static ABI size_t guest_strlen(const char *text) { return strlen(text); }
static ABI __attribute__((noreturn)) void guest_libc_exit(int status) {
    printf("Runtime: guest requested exit(%d)\n", status);
    runtime_finalize(NULL);
    runtime_report();
    exit(status);
}
/* Imports are spelled NID#library by link_modules.py, the same for every game. libScePosix
 * exports libkernel's symbols with the same contracts, so both share the libkernel key. */
static const char *canonical_name(const char *name, char out[128]) {
    const char *posix=strstr(name,"#libScePosix");
    if (!posix || posix[12] || posix-name>100) return name;
    snprintf(out,128,"%.*s#libkernel",(int)(posix-name),name);
    return out;
}
uintptr_t runtime_resolve(const char *name, int is_data) {
    if (!(capabilities & 1)) return 0;
    char canonical[128];
    name=canonical_name(name,canonical);
    if (is_data) {
        static int32_t need_libc_internal = 1; /* SDK marker variable referenced by Fios2 */
        if (!strcmp(name, "f7uOxY9mM1U#libkernel")) return (uintptr_t)&stack_canary;
        if (!strcmp(name, "ZT4ODD2Ts9o#libSceLibcInternal")) return (uintptr_t)&need_libc_internal;
        static const char *progname = "eboot.bin";
        if (!strcmp(name, "djxxOmW6-aw#libkernel")) return (uintptr_t)&progname;
        return 0;
    }
    /* Exact imports; the suffix is the exporting library. */
    if (!strcmp(name, "bzQExy189ZI#libc")) return (uintptr_t)init_env;
    if (!strcmp(name, "8G2LB+A3rzg#libc")) return (uintptr_t)guest_atexit;
    if (!strcmp(name, "tsvEmnenz48#libc")) return (uintptr_t)guest_cxa_atexit;
    if (!strcmp(name, "uMei1W9uyNo#libc")) return (uintptr_t)guest_libc_exit;
    /* Not imported by the current eboot, exposed for ABI tests/future use. */
    if (!strcmp(name, "H2e8t5ScQGc#libc")) return (uintptr_t)guest_finalize;
    if (!strcmp(name, "3GPpjQdAMTw#libc")) return (uintptr_t)guard_acquire;
    if (!strcmp(name, "9rAeANT2tyE#libc")) return (uintptr_t)guard_release;
    if (!strcmp(name, "2emaaluWzUw#libc")) return (uintptr_t)guard_abort;
    if (!strcmp(name, "Ou3iL1abvng#libkernel")) return (uintptr_t)stack_fail;
    if (!strcmp(name, "8zTFvBIAIN8#libc")) return (uintptr_t)guest_memset;
    if (!strcmp(name, "Q3VBxCXhUHs#libc")) return (uintptr_t)guest_memcpy;
    if (!strcmp(name, "+P6FRGH4LfA#libc")) return (uintptr_t)guest_memmove;
    if (!strcmp(name, "DfivPArhucg#libc")) return (uintptr_t)guest_memcmp;
    if (!strcmp(name, "j4ViWNHEgww#libc")) return (uintptr_t)guest_strlen;
    if (!strcmp(name, "vNe1w4diLCs#libkernel")) return (uintptr_t)guest_tls_get_addr;
    if (!strcmp(name, "959qrazPIrg#libkernel")) return (uintptr_t)guest_procparam;
    if (!strcmp(name, "p5EcQeEeJAE#libkernel")) return (uintptr_t)guest_set_heap_api;
    uintptr_t mutex = runtime_mutex_resolve(name);
    if (mutex) return mutex;
    uintptr_t thread = runtime_thread_resolve(name);
    if (thread) return thread;
    uintptr_t content=runtime_content_resolve(name);
    if (content) return content;
    uintptr_t timer = runtime_time_resolve(name);
    if (timer) return timer;
    uintptr_t sema = runtime_sema_resolve(name);
    if (sema) return sema;
    uintptr_t rwlock = runtime_rwlock_resolve(name);
    if (rwlock) return rwlock;
    uintptr_t memory = runtime_memory_resolve(name);
    if (memory) return memory;
    uintptr_t kernel = runtime_kernel_resolve(name);
    if (kernel) return kernel;
    uintptr_t services = runtime_services_resolve(name);
    if (services) return services;
    uintptr_t ajm = runtime_ajm_resolve(name);
    if (ajm) return ajm;
    uintptr_t videodec = runtime_videodec_resolve(name);
    if (videodec) return videodec;
    uintptr_t audio = runtime_audio_resolve(name);
    if (audio) return audio;
    uintptr_t pad = runtime_pad_resolve(name);
    if (pad) return pad;
    uintptr_t rtc = runtime_rtc_resolve(name);
    if (rtc) return rtc;
    uintptr_t save = runtime_savedata_resolve(name);
    if (save) return save;
    uintptr_t file = runtime_file_resolve(name);
    if (file) return file;
    return bbgpu_resolve(name);
}
static const struct { const char *nid, *symbol; } import_names[]={
#include "import_names.inc"
};
/* Symbol name for a scoped NID, or NULL when this eboot/libc never imports it. */
const char *runtime_symbol(const char *nid) {
    for (size_t i=0;i<sizeof(import_names)/sizeof(*import_names);++i)
        if (!strcmp(nid,import_names[i].nid)) return import_names[i].symbol;
    return NULL;
}
/* Same symbol name means same contract whether imported from libkernel or
 * libScePosix, so newer modules resolve by name through runtime_symbol. */
uintptr_t runtime_lookup(const RuntimeExport *table,size_t count,const char *nid) {
    const char *symbol=runtime_symbol(nid);
    if (!symbol) return 0;
    for (size_t i=0;i<count;++i) if (!strcmp(symbol,table[i].name)) return (uintptr_t)table[i].function;
    return 0;
}
const char *runtime_import_name(const char *name) {
    char canonical[128];
    const char *symbol=runtime_symbol(canonical_name(name,canonical));
    return symbol ? symbol : "name not resolved; see analysis.json import_name_hints";
}
