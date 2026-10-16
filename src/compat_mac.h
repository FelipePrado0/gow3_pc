/* macOS (Apple Clang / Darwin) counterparts of Linux/POSIX calls the
 * runtime uses. Included through runtime.h after C library headers. */
#ifndef GOW3_COMPAT_MAC_H
#define GOW3_COMPAT_MAC_H

#ifdef __APPLE__
#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>
#include <time.h>
#include <errno.h>
#include <fcntl.h>
#include <unistd.h>
#include <pthread.h>
#include <sys/types.h>
#include <sys/stat.h>
#include <sys/mman.h>

#ifndef CLOCK_REALTIME_COARSE
#define CLOCK_REALTIME_COARSE CLOCK_REALTIME
#endif

#ifndef RUSAGE_THREAD
#define RUSAGE_THREAD RUSAGE_SELF
#endif

#ifndef MAP_ANONYMOUS
#define MAP_ANONYMOUS MAP_ANON
#endif

#ifndef MAP_NORESERVE
#define MAP_NORESERVE 0
#endif

#ifndef st_atim
#define st_atim st_atimespec
#endif
#ifndef st_mtim
#define st_mtim st_mtimespec
#endif
#ifndef st_ctim
#define st_ctim st_ctimespec
#endif

#ifndef PTHREAD_RECURSIVE_MUTEX_INITIALIZER_NP
#define PTHREAD_RECURSIVE_MUTEX_INITIALIZER_NP PTHREAD_RECURSIVE_MUTEX_INITIALIZER
#endif

#ifndef TIMER_ABSTIME
#define TIMER_ABSTIME 1
#endif

static inline int compat_clock_nanosleep(clockid_t clock_id, int flags, const struct timespec *request, struct timespec *remain) {
    (void)remain;
    if (flags == TIMER_ABSTIME) {
        struct timespec now;
        clock_gettime(clock_id, &now);
        if (request->tv_sec < now.tv_sec || (request->tv_sec == now.tv_sec && request->tv_nsec <= now.tv_nsec)) return 0;
        uint64_t req_ns = (uint64_t)request->tv_sec * 1000000000ULL + (uint64_t)request->tv_nsec;
        uint64_t now_ns = (uint64_t)now.tv_sec * 1000000000ULL + (uint64_t)now.tv_nsec;
        uint64_t diff = req_ns - now_ns;
        struct timespec req = {(time_t)(diff / 1000000000ULL), (long)(diff % 1000000000ULL)};
        return nanosleep(&req, NULL);
    } else {
        return nanosleep(request, NULL);
    }
}
#define clock_nanosleep compat_clock_nanosleep

static inline ssize_t compat_getrandom(void *buf, size_t buflen, unsigned int flags) {
    (void)flags;
    arc4random_buf(buf, buflen);
    return (ssize_t)buflen;
}
#define getrandom compat_getrandom

static inline int compat_pthread_mutex_timedlock(pthread_mutex_t *mutex, const struct timespec *abs_timeout) {
    int res;
    while ((res = pthread_mutex_trylock(mutex)) == EBUSY) {
        struct timespec now;
        clock_gettime(CLOCK_REALTIME, &now);
        if (now.tv_sec > abs_timeout->tv_sec ||
            (now.tv_sec == abs_timeout->tv_sec && now.tv_nsec >= abs_timeout->tv_nsec)) {
            return ETIMEDOUT;
        }
        struct timespec sleep_ts = {0, 100000}; /* 100 µs */
        nanosleep(&sleep_ts, NULL);
    }
    return res;
}
#define pthread_mutex_timedlock compat_pthread_mutex_timedlock

static inline int compat_pthread_rwlock_timedrdlock(pthread_rwlock_t *rwlock, const struct timespec *abs_timeout) {
    int res;
    while ((res = pthread_rwlock_tryrdlock(rwlock)) == EBUSY) {
        struct timespec now;
        clock_gettime(CLOCK_REALTIME, &now);
        if (now.tv_sec > abs_timeout->tv_sec ||
            (now.tv_sec == abs_timeout->tv_sec && now.tv_nsec >= abs_timeout->tv_nsec)) {
            return ETIMEDOUT;
        }
        struct timespec sleep_ts = {0, 100000};
        nanosleep(&sleep_ts, NULL);
    }
    return res;
}
#define pthread_rwlock_timedrdlock compat_pthread_rwlock_timedrdlock

static inline int compat_pthread_rwlock_timedwrlock(pthread_rwlock_t *rwlock, const struct timespec *abs_timeout) {
    int res;
    while ((res = pthread_rwlock_trywrlock(rwlock)) == EBUSY) {
        struct timespec now;
        clock_gettime(CLOCK_REALTIME, &now);
        if (now.tv_sec > abs_timeout->tv_sec ||
            (now.tv_sec == abs_timeout->tv_sec && now.tv_nsec >= abs_timeout->tv_nsec)) {
            return ETIMEDOUT;
        }
        struct timespec sleep_ts = {0, 100000};
        nanosleep(&sleep_ts, NULL);
    }
    return res;
}
#define pthread_rwlock_timedwrlock compat_pthread_rwlock_timedwrlock

/* pthread_barrier implementation for macOS */
typedef struct {
    pthread_mutex_t mutex;
    pthread_cond_t cond;
    unsigned int count;
    unsigned int trip_count;
    unsigned int cycle;
} compat_barrier_t;

#ifndef PTHREAD_BARRIER_SERIAL_THREAD
#define PTHREAD_BARRIER_SERIAL_THREAD 1
#endif
#define pthread_barrier_t compat_barrier_t

static inline int compat_pthread_barrier_init(pthread_barrier_t *barrier, const void *attr, unsigned int count) {
    (void)attr;
    if (count == 0) return EINVAL;
    pthread_mutex_init(&barrier->mutex, NULL);
    pthread_cond_init(&barrier->cond, NULL);
    barrier->count = count;
    barrier->trip_count = count;
    barrier->cycle = 0;
    return 0;
}
#define pthread_barrier_init compat_pthread_barrier_init

static inline int compat_pthread_barrier_wait(pthread_barrier_t *barrier) {
    pthread_mutex_lock(&barrier->mutex);
    unsigned int cycle = barrier->cycle;
    if (--barrier->trip_count == 0) {
        barrier->cycle++;
        barrier->trip_count = barrier->count;
        pthread_cond_broadcast(&barrier->cond);
        pthread_mutex_unlock(&barrier->mutex);
        return PTHREAD_BARRIER_SERIAL_THREAD;
    }
    while (cycle == barrier->cycle) {
        pthread_cond_wait(&barrier->cond, &barrier->mutex);
    }
    pthread_mutex_unlock(&barrier->mutex);
    return 0;
}
#define pthread_barrier_wait compat_pthread_barrier_wait

static inline int compat_pthread_barrier_destroy(pthread_barrier_t *barrier) {
    pthread_mutex_destroy(&barrier->mutex);
    pthread_cond_destroy(&barrier->cond);
    return 0;
}
#define pthread_barrier_destroy compat_pthread_barrier_destroy

#endif /* __APPLE__ */
#endif /* GOW3_COMPAT_MAC_H */
