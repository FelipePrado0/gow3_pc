/* macOS counterparts of the Linux calls the runtime uses. Included through runtime.h after the C
 * library headers, so the macros below only replace calls in this project's own sources. The
 * process is x86-64: native on Intel Macs, Rosetta 2 on Apple Silicon. */
#ifndef GOW3_COMPAT_MAC_H
#define GOW3_COMPAT_MAC_H
#ifdef __APPLE__
#include <errno.h>
#include <pthread.h>
#include <stdint.h>
#include <stddef.h>
#include <time.h>

#ifndef CLOCK_REALTIME_COARSE
#define CLOCK_REALTIME_COARSE CLOCK_REALTIME
#endif
#ifndef PTHREAD_RECURSIVE_MUTEX_INITIALIZER_NP
#define PTHREAD_RECURSIVE_MUTEX_INITIALIZER_NP PTHREAD_RECURSIVE_MUTEX_INITIALIZER
#endif

/* No timed mutex or rwlock waits on macOS: try-lock until the CLOCK_REALTIME deadline. */
int compat_mutex_timedlock(pthread_mutex_t *mutex, const struct timespec *deadline);
int compat_rwlock_timedrdlock(pthread_rwlock_t *lock, const struct timespec *deadline);
int compat_rwlock_timedwrlock(pthread_rwlock_t *lock, const struct timespec *deadline);
#define pthread_mutex_timedlock compat_mutex_timedlock
#define pthread_rwlock_timedrdlock compat_rwlock_timedrdlock
#define pthread_rwlock_timedwrlock compat_rwlock_timedwrlock
/* Condition variables only wait for CLOCK_REALTIME deadlines. */
static inline int compat_condattr_setclock(pthread_condattr_t *attr, clockid_t clock) {
    (void)attr; return clock == CLOCK_REALTIME ? 0 : EINVAL;
}
#define pthread_condattr_setclock compat_condattr_setclock
/* Sleeps with sub-millisecond precision: macOS lets ordinary timers (nanosleep, condition
 * variables) fire up to a third of their length late (timer coalescing). */
void compat_sleep_ns(uint64_t ns);
/* Sleeps until a CLOCK_MONOTONIC time (clock_nanosleep with TIMER_ABSTIME), as precisely. */
void compat_sleep_until_monotonic(uint64_t deadline_ns);
/* User and kernel CPU time of the process (who=0) or calling thread (who=1), microseconds. */
int compat_cpu_times(int who, int64_t *user_us, int64_t *kernel_us);
/* System-wide id of the calling thread (Linux gettid). */
uint64_t compat_thread_id(void);
/* The executable's directory, without a trailing slash; "" when unknown. */
const char *compat_executable_dir(void);
/* Points the Vulkan loader at the KosmicKrisp driver next to the executable
 * (vulkan/kosmickrisp_mesa_icd.json) unless VK_DRIVER_FILES or VK_ICD_FILENAMES is set. */
void compat_vulkan_driver(void);
#endif
#endif
