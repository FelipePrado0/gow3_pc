/* GPU library start without the game: the window, the Vulkan instance and device, the presenter
 * and the GPU threads come up as the loader brings them up (gow3gpu_init), frames are presented
 * for a moment, then the process exits. Needs a display. Usage: gpu-start-test [seconds [width height]] */
#include "runtime.h"
#include "gpu/gow3gpu.h"
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

static unsigned seconds = 3;
static int32_t width = 1280, height = 720;

static void *start(void *unused) {
    (void)unused;
    const char *user = getenv("GOW3_GPU_USER_DIR");
    gow3gpu_register_kernel();
    Gow3GpuConfig config = {"God of War III (GPU start test)", "CUSA01623", user ? user : "user", 0x04508101u, 0,
                            width, height};
    if (gow3gpu_init(&config)) {
        puts("GPU start: FAIL (gow3gpu_init)");
        exit(1);
    }
    printf("GPU start: window, Vulkan device and presenter ready (%u HLE symbols)\n", gow3gpu_symbol_count());
    sleep(seconds);
    puts("GPU start: PASS");
    fflush(NULL);
    _exit(0); /* GPU threads are running: no destructors, as the loader ends */
}

int main(int argc, char **argv) {
    setvbuf(stdout, NULL, _IONBF, 0);
    if (argc > 1) seconds = (unsigned)strtoul(argv[1], NULL, 10);
    if (argc > 3) {
        width = (int32_t)strtol(argv[2], NULL, 10);
        height = (int32_t)strtol(argv[3], NULL, 10);
    }
#ifdef __APPLE__
    compat_vulkan_driver();
    pthread_t thread;
    if (pthread_create(&thread, NULL, start, NULL)) return 1;
    gow3gpu_main_loop(); /* the window lives on the main thread */
#else
    start(NULL);
    return 0;
#endif
}
