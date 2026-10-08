/* Windows (MSYS2 CLANG64, MinGW-w64 + winpthreads) counterparts of the POSIX calls the
 * runtime uses. Included through runtime.h after the C library headers, so the macros below
 * only replace calls in this project's own sources. Paths are UTF-8: the executable's
 * manifest selects the UTF-8 code page (packaging/windows/gow3-probe.manifest). */
#ifndef GOW3_COMPAT_WIN_H
#define GOW3_COMPAT_WIN_H
#ifdef _WIN32
#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>
#include <time.h>
#include <errno.h>
#include <fcntl.h>
#include <io.h>
#include <direct.h>
#include <unistd.h>
#include <sys/types.h>
#include <sys/stat.h>

#ifndef O_NONBLOCK
#define O_NONBLOCK 0
#endif
#ifndef O_SYNC
#define O_SYNC 0
#endif
#ifndef O_CLOEXEC
#define O_CLOEXEC _O_NOINHERIT
#endif
#ifndef PATH_MAX
#define PATH_MAX 4096
#endif
#ifndef CLOCK_REALTIME_COARSE
#define CLOCK_REALTIME_COARSE CLOCK_REALTIME
#endif
/* struct tm without tm_gmtoff: see compat_utc_offset. */
#define localtime_r(time, out) (localtime_s((out), (time)) ? NULL : (out))
#ifndef PTHREAD_RECURSIVE_MUTEX_INITIALIZER_NP
#define PTHREAD_RECURSIVE_MUTEX_INITIALIZER_NP PTHREAD_RECURSIVE_MUTEX_INITIALIZER
#endif
/* Protection bits for runtime_low_map and the memory backend (mapped to PAGE_* there). */
#ifndef PROT_NONE
#define PROT_NONE 0
#define PROT_READ 1
#define PROT_WRITE 2
#define PROT_EXEC 4
#endif

/* POSIX mkdir has a mode; the CRT's does not. */
static inline int compat_mkdir(const char *path, int mode) { (void)mode; return _mkdir(path); }
#define mkdir(path, mode) compat_mkdir(path, mode)

/* pread/pwrite on CRT descriptors: positioned I/O that leaves the file offset unchanged. */
ssize_t compat_pread(int fd, void *buffer, size_t size, int64_t offset);
ssize_t compat_pwrite(int fd, const void *buffer, size_t size, int64_t offset);
#define pread compat_pread
#define pwrite compat_pwrite
static inline int compat_fsync(int fd) { return _commit(fd); }
#define fsync compat_fsync
static inline char *compat_realpath(const char *path, char *out) { return _fullpath(out, path, PATH_MAX); }
#define realpath compat_realpath

/* Seconds east of UTC at time t (struct tm has no tm_gmtoff on Windows). */
long compat_utc_offset(time_t t);
/* Cryptographic random bytes; 0 or -1. */
int compat_random(void *buffer, size_t size);
/* Sleeps with sub-millisecond precision (high-resolution waitable timer; winpthreads'
 * nanosleep rounds up to the 15.6 ms scheduler tick). */
void compat_sleep_ns(uint64_t ns);
/* rm -rf: 0 when the tree is gone (or never existed). */
int compat_remove_tree(const char *path);
/* errno for a Win32 error code. */
int compat_errno_from_win32(unsigned long error);
/* User and kernel CPU time of the process (who=0) or calling thread (who=1), microseconds. */
int compat_cpu_times(int who, int64_t *user_us, int64_t *kernel_us);
/* setjmp/longjmp without SEH unwinding (guest frames have no unwind data) that saves every
 * Win64 callee-saved register, xmm6-15 included. clang's __builtin_longjmp restores a wrong
 * frame pointer on Win64. gow3_longjmp may also be entered from the exception handler. */
typedef unsigned long long gow3_jmp_buf[32];
__attribute__((returns_twice)) int gow3_setjmp(gow3_jmp_buf buffer);
__attribute__((noreturn)) void gow3_longjmp(gow3_jmp_buf buffer, int value);
#endif
#endif
