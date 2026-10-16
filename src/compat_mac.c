/* macOS implementations of compat_mac.h. */
#ifdef __APPLE__
#include "compat_mac.h"
#include <fcntl.h>
#include <limits.h>
#include <mach/mach.h>
#include <mach-o/dyld.h>
#include <sched.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/event.h>
#include <sys/resource.h>
#include <unistd.h>

static uint64_t realtime_ns(void) {
    struct timespec t;
    clock_gettime(CLOCK_REALTIME, &t);
    return (uint64_t)t.tv_sec * 1000000000u + (uint64_t)t.tv_nsec;
}
static void nap_ns(uint64_t ns) {
    struct timespec t = {(time_t)(ns / 1000000000u), (long)(ns % 1000000000u)};
    while (nanosleep(&t, &t) && errno == EINTR) {}
}
/* Polls `attempt` (a try-lock: 0 or EBUSY) until it succeeds or the deadline passes. Waits start
 * at 20 us and double up to 1 ms, so short holds are caught quickly and long waits stay cheap. */
static int timed(int (*attempt)(void *), void *object, const struct timespec *deadline) {
    if (!deadline || deadline->tv_nsec < 0 || deadline->tv_nsec >= 1000000000) return EINVAL;
    const uint64_t end = (uint64_t)deadline->tv_sec * 1000000000u + (uint64_t)deadline->tv_nsec;
    for (uint64_t wait = 20000;; wait = wait < 1000000 ? wait * 2 : 1000000) {
        const int e = attempt(object);
        if (e != EBUSY) return e;
        const uint64_t now = realtime_ns();
        if (now >= end) return ETIMEDOUT;
        nap_ns(end - now < wait ? end - now : wait);
    }
}
static int try_mutex(void *m) { return pthread_mutex_trylock(m); }
static int try_read(void *l) { return pthread_rwlock_tryrdlock(l); }
static int try_write(void *l) { return pthread_rwlock_trywrlock(l); }
int compat_mutex_timedlock(pthread_mutex_t *mutex, const struct timespec *deadline) {
    return timed(try_mutex, mutex, deadline);
}
int compat_rwlock_timedrdlock(pthread_rwlock_t *lock, const struct timespec *deadline) {
    return timed(try_read, lock, deadline);
}
int compat_rwlock_timedwrlock(pthread_rwlock_t *lock, const struct timespec *deadline) {
    return timed(try_write, lock, deadline);
}

/* A kqueue timer marked NOTE_CRITICAL is exempt from timer coalescing (about 0.1 ms late
 * instead of a third of the wait). One queue per thread, closed when the thread ends. */
static pthread_key_t queue_key;
static pthread_once_t queue_once = PTHREAD_ONCE_INIT;
static void close_queue(void *value) { close((int)(intptr_t)value - 1); }
static void create_queue_key(void) { pthread_key_create(&queue_key, close_queue); }
static int thread_queue(void) {
    pthread_once(&queue_once, create_queue_key);
    const intptr_t value = (intptr_t)pthread_getspecific(queue_key);
    if (value) return (int)value - 1;
    const int queue = kqueue();
    if (queue >= 0) {
        fcntl(queue, F_SETFD, FD_CLOEXEC);
        pthread_setspecific(queue_key, (void *)(intptr_t)(queue + 1));
    }
    return queue;
}
static uint64_t monotonic_ns(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (uint64_t)t.tv_sec * 1000000000u + (uint64_t)t.tv_nsec;
}
void compat_sleep_until_monotonic(uint64_t deadline_ns) {
    for (uint64_t now = monotonic_ns(); now < deadline_ns; now = monotonic_ns()) {
        const int queue = thread_queue();
        struct kevent change, event;
        EV_SET(&change, 1, EVFILT_TIMER, EV_ADD | EV_ONESHOT, NOTE_NSECONDS | NOTE_CRITICAL,
               (intptr_t)(deadline_ns - now), NULL);
        if (queue < 0 || (kevent(queue, &change, 1, &event, 1, NULL) < 0 && errno != EINTR))
            nap_ns(deadline_ns - now);
    }
}
void compat_sleep_ns(uint64_t ns) {
    if (!ns) { sched_yield(); return; }
    compat_sleep_until_monotonic(monotonic_ns() + ns);
}

int compat_cpu_times(int who, int64_t *user_us, int64_t *kernel_us) {
    if (who == 0) {
        struct rusage r;
        if (getrusage(RUSAGE_SELF, &r)) return -1;
        *user_us = (int64_t)r.ru_utime.tv_sec * 1000000 + r.ru_utime.tv_usec;
        *kernel_us = (int64_t)r.ru_stime.tv_sec * 1000000 + r.ru_stime.tv_usec;
        return 0;
    }
    thread_basic_info_data_t info;
    mach_msg_type_number_t count = THREAD_BASIC_INFO_COUNT;
    const mach_port_t self = mach_thread_self();
    const kern_return_t r = thread_info(self, THREAD_BASIC_INFO, (thread_info_t)&info, &count);
    mach_port_deallocate(mach_task_self(), self);
    if (r != KERN_SUCCESS) return -1;
    *user_us = (int64_t)info.user_time.seconds * 1000000 + info.user_time.microseconds;
    *kernel_us = (int64_t)info.system_time.seconds * 1000000 + info.system_time.microseconds;
    return 0;
}

uint64_t compat_thread_id(void) {
    uint64_t id = 0;
    pthread_threadid_np(NULL, &id);
    return id;
}

static char executable_dir[PATH_MAX];
static void find_executable_dir(void) {
    char path[PATH_MAX], real[PATH_MAX];
    uint32_t size = sizeof(path);
    if (_NSGetExecutablePath(path, &size)) return;
    snprintf(executable_dir, sizeof(executable_dir), "%s", realpath(path, real) ? real : path);
    char *slash = strrchr(executable_dir, '/');
    if (slash) *slash = 0;
    else executable_dir[0] = 0;
}
const char *compat_executable_dir(void) {
    static pthread_once_t once = PTHREAD_ONCE_INIT;
    pthread_once(&once, find_executable_dir);
    return executable_dir;
}

void compat_vulkan_driver(void) {
    if (getenv("VK_DRIVER_FILES") || getenv("VK_ICD_FILENAMES")) return;
    char icd[PATH_MAX];
    snprintf(icd, sizeof(icd), "%s/vulkan/kosmickrisp_mesa_icd.json", compat_executable_dir());
    if (!access(icd, R_OK)) setenv("VK_DRIVER_FILES", icd, 1);
    else fprintf(stderr, "Vulkan: %s is missing (run build.sh); using the system's Vulkan drivers\n", icd);
}
#endif
