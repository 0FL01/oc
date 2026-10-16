/* Fixture-only pre-effect barrier. Loaded only into an owned native oc process. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <fcntl.h>
#include <stdlib.h>
#include <stdio.h>
#include <sys/syscall.h>
#include <unistd.h>

static void barrier(void) {
    const char *arm = getenv("T50_FORK_ARM");
    const char *reached = getenv("T50_FORK_REACHED");
    if (!arm || !reached || syscall(SYS_access, arm, F_OK) != 0) return;
    long fd = syscall(SYS_openat, AT_FDCWD, reached,
                      O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC, 0600);
    if (fd < 0 || syscall(SYS_write, fd, "f", 1) != 1) _exit(91);
    syscall(SYS_close, fd);
    for (;;) syscall(SYS_pause);
}

pid_t fork(void) {
    barrier();
    pid_t (*next)(void) = dlsym(RTLD_NEXT, "fork");
    if (!next) _exit(92);
    pid_t pid = next();
    const char *record = getenv("T50_FORK_PID");
    if (pid > 0 && record) {
        char text[32];
        int size = snprintf(text, sizeof(text), "%d", (int)pid);
        long fd = syscall(SYS_openat, AT_FDCWD, record, O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC, 0600);
        if (fd < 0 || syscall(SYS_write, fd, text, (size_t)size) != size) _exit(94);
        syscall(SYS_close, fd);
    }
    return pid;
}

pid_t _Fork(void) {
    barrier();
    pid_t (*next)(void) = dlsym(RTLD_NEXT, "_Fork");
    if (!next) _exit(93);
    return next();
}
