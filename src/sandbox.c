#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <fcntl.h>
#include <dlfcn.h>
#include <stdarg.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <limits.h>
#include <errno.h>

static char g_game_dir[PATH_MAX] = {0};
static size_t g_game_dir_len = 0;
static char g_mutable_dirs[16][64];
static int g_mutable_dirs_count = 0;
static int g_initialized = 0;

static int (*real_open)(const char *pathname, int flags, ...) = NULL;
static int (*real_open64)(const char *pathname, int flags, ...) = NULL;
static int (*real_openat)(int dirfd, const char *pathname, int flags, ...) = NULL;
static int (*real_openat64)(int dirfd, const char *pathname, int flags, ...) = NULL;
static int (*real_creat)(const char *pathname, mode_t mode) = NULL;
static int (*real_creat64)(const char *pathname, mode_t mode) = NULL;
static FILE *(*real_fopen)(const char *pathname, const char *mode) = NULL;
static FILE *(*real_fopen64)(const char *pathname, const char *mode) = NULL;
static int (*real_unlink)(const char *pathname) = NULL;
static int (*real_unlinkat)(int dirfd, const char *pathname, int flags) = NULL;
static int (*real_remove)(const char *pathname) = NULL;
static int (*real_rename)(const char *oldpath, const char *newpath) = NULL;
static int (*real_renameat)(int olddirfd, const char *oldpath, int newdirfd, const char *newpath) = NULL;
static int (*real_renameat2)(int olddirfd, const char *oldpath, int newdirfd, const char *newpath, unsigned int flags) = NULL;
static int (*real_mkdir)(const char *pathname, mode_t mode) = NULL;
static int (*real_mkdirat)(int dirfd, const char *pathname, mode_t mode) = NULL;
static int (*real_rmdir)(const char *pathname) = NULL;
static int (*real_chmod)(const char *pathname, mode_t mode) = NULL;
static int (*real_fchmodat)(int dirfd, const char *pathname, mode_t mode, int flags) = NULL;

static void init_symbols(void) {
    if (g_initialized) return;
    g_initialized = 1;

    real_open = dlsym(RTLD_NEXT, "open");
    real_open64 = dlsym(RTLD_NEXT, "open64");
    if (!real_open64) real_open64 = real_open;

    real_openat = dlsym(RTLD_NEXT, "openat");
    real_openat64 = dlsym(RTLD_NEXT, "openat64");
    if (!real_openat64) real_openat64 = real_openat;

    real_creat = dlsym(RTLD_NEXT, "creat");
    real_creat64 = dlsym(RTLD_NEXT, "creat64");
    if (!real_creat64) real_creat64 = real_creat;

    real_fopen = dlsym(RTLD_NEXT, "fopen");
    real_fopen64 = dlsym(RTLD_NEXT, "fopen64");
    if (!real_fopen64) real_fopen64 = real_fopen;

    real_unlink = dlsym(RTLD_NEXT, "unlink");
    real_unlinkat = dlsym(RTLD_NEXT, "unlinkat");
    real_remove = dlsym(RTLD_NEXT, "remove");

    real_rename = dlsym(RTLD_NEXT, "rename");
    real_renameat = dlsym(RTLD_NEXT, "renameat");
    real_renameat2 = dlsym(RTLD_NEXT, "renameat2");

    real_mkdir = dlsym(RTLD_NEXT, "mkdir");
    real_mkdirat = dlsym(RTLD_NEXT, "mkdirat");
    real_rmdir = dlsym(RTLD_NEXT, "rmdir");

    real_chmod = dlsym(RTLD_NEXT, "chmod");
    real_fchmodat = dlsym(RTLD_NEXT, "fchmodat");

    const char *env_game_dir = getenv("NITE_GAME_DIR");
    if (env_game_dir && env_game_dir[0] != '\0') {
        char resolved[PATH_MAX];
        if (realpath(env_game_dir, resolved)) {
            snprintf(g_game_dir, sizeof(g_game_dir), "%s", resolved);
        } else {
            snprintf(g_game_dir, sizeof(g_game_dir), "%s", env_game_dir);
        }
        g_game_dir_len = strlen(g_game_dir);
    }

    const char *env_mut = getenv("NITE_MUTABLE_DIRS");
    if (!env_mut) {
        env_mut = "saves:screenshots:logs:xaero:XaeroWorldMap:XaeroWaypoints";
    }

    char buf[512];
    strncpy(buf, env_mut, sizeof(buf) - 1);
    buf[sizeof(buf) - 1] = '\0';

    char *token = strtok(buf, ":");
    while (token && g_mutable_dirs_count < 16) {
        strncpy(g_mutable_dirs[g_mutable_dirs_count], token, 63);
        g_mutable_dirs[g_mutable_dirs_count][63] = '\0';
        g_mutable_dirs_count++;
        token = strtok(NULL, ":");
    }
}

// Returns 1 if path is inside game_dir AND NOT in a mutable directory.
static int is_immutable_target(int dirfd, const char *path) {
    if (!path || path[0] == '\0') return 0;
    if (g_game_dir_len == 0) return 0;

    char resolved[PATH_MAX];
    resolved[0] = '\0';

    if (path[0] == '/') {
        strncpy(resolved, path, PATH_MAX - 1);
        resolved[PATH_MAX - 1] = '\0';
    } else {
        if (dirfd == AT_FDCWD || dirfd < 0) {
            if (getcwd(resolved, PATH_MAX) != NULL) {
                size_t clen = strlen(resolved);
                if (clen + 1 + strlen(path) < PATH_MAX) {
                    strcat(resolved, "/");
                    strcat(resolved, path);
                }
            }
        } else {
            char procfd[64];
            snprintf(procfd, sizeof(procfd), "/proc/self/fd/%d", dirfd);
            ssize_t len = readlink(procfd, resolved, PATH_MAX - 1);
            if (len > 0) {
                resolved[len] = '\0';
                if (len + 1 + (ssize_t)strlen(path) < PATH_MAX) {
                    strcat(resolved, "/");
                    strcat(resolved, path);
                }
            }
        }
    }

    if (resolved[0] == '\0') return 0;

    // Check if path starts with g_game_dir
    if (strncmp(resolved, g_game_dir, g_game_dir_len) != 0) {
        return 0; // outside game dir
    }

    const char *rel = resolved + g_game_dir_len;
    while (*rel == '/') rel++;

    if (*rel == '\0') {
        // the game dir itself
        return 1;
    }

    // Check if the relative path starts with any allowed mutable directory
    for (int i = 0; i < g_mutable_dirs_count; ++i) {
        size_t mlen = strlen(g_mutable_dirs[i]);
        if (strncmp(rel, g_mutable_dirs[i], mlen) == 0) {
            char next_ch = rel[mlen];
            if (next_ch == '\0' || next_ch == '/') {
                return 0; // matches a mutable directory -> allow real write!
            }
        }
    }

    // Inside game_dir, but NOT in a mutable directory -> immutable!
    return 1;
}

static int open_dev_null(int flags) {
    if (!real_open) init_symbols();
    return real_open("/dev/null", (flags & ~O_CREAT & ~O_EXCL & ~O_TRUNC));
}

int open(const char *pathname, int flags, ...) {
    init_symbols();
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list args;
        va_start(args, flags);
        mode = va_arg(args, mode_t);
        va_end(args);
    }

    int is_write = ((flags & O_ACCMODE) != O_RDONLY) || (flags & (O_CREAT | O_TRUNC | O_APPEND));
    if (is_write && is_immutable_target(AT_FDCWD, pathname)) {
        return open_dev_null(flags);
    }
    return real_open(pathname, flags, mode);
}

int open64(const char *pathname, int flags, ...) {
    init_symbols();
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list args;
        va_start(args, flags);
        mode = va_arg(args, mode_t);
        va_end(args);
    }

    int is_write = ((flags & O_ACCMODE) != O_RDONLY) || (flags & (O_CREAT | O_TRUNC | O_APPEND));
    if (is_write && is_immutable_target(AT_FDCWD, pathname)) {
        return open_dev_null(flags);
    }
    return real_open64(pathname, flags, mode);
}

int openat(int dirfd, const char *pathname, int flags, ...) {
    init_symbols();
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list args;
        va_start(args, flags);
        mode = va_arg(args, mode_t);
        va_end(args);
    }

    int is_write = ((flags & O_ACCMODE) != O_RDONLY) || (flags & (O_CREAT | O_TRUNC | O_APPEND));
    if (is_write && is_immutable_target(dirfd, pathname)) {
        return open_dev_null(flags);
    }
    return real_openat(dirfd, pathname, flags, mode);
}

int openat64(int dirfd, const char *pathname, int flags, ...) {
    init_symbols();
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list args;
        va_start(args, flags);
        mode = va_arg(args, mode_t);
        va_end(args);
    }

    int is_write = ((flags & O_ACCMODE) != O_RDONLY) || (flags & (O_CREAT | O_TRUNC | O_APPEND));
    if (is_write && is_immutable_target(dirfd, pathname)) {
        return open_dev_null(flags);
    }
    return real_openat64(dirfd, pathname, flags, mode);
}

int creat(const char *pathname, mode_t mode) {
    init_symbols();
    if (is_immutable_target(AT_FDCWD, pathname)) {
        return open_dev_null(O_WRONLY);
    }
    return real_creat(pathname, mode);
}

int creat64(const char *pathname, mode_t mode) {
    init_symbols();
    if (is_immutable_target(AT_FDCWD, pathname)) {
        return open_dev_null(O_WRONLY);
    }
    return real_creat64(pathname, mode);
}

FILE *fopen(const char *pathname, const char *mode) {
    init_symbols();
    int is_write = (strchr(mode, 'w') != NULL || strchr(mode, 'a') != NULL || strchr(mode, '+') != NULL);
    if (is_write && is_immutable_target(AT_FDCWD, pathname)) {
        return real_fopen("/dev/null", mode);
    }
    return real_fopen(pathname, mode);
}

FILE *fopen64(const char *pathname, const char *mode) {
    init_symbols();
    int is_write = (strchr(mode, 'w') != NULL || strchr(mode, 'a') != NULL || strchr(mode, '+') != NULL);
    if (is_write && is_immutable_target(AT_FDCWD, pathname)) {
        return real_fopen64("/dev/null", mode);
    }
    return real_fopen64(pathname, mode);
}

int unlink(const char *pathname) {
    init_symbols();
    if (is_immutable_target(AT_FDCWD, pathname)) {
        return 0; // sure thing buddy
    }
    return real_unlink(pathname);
}

int unlinkat(int dirfd, const char *pathname, int flags) {
    init_symbols();
    if (is_immutable_target(dirfd, pathname)) {
        return 0; // sure thing buddy
    }
    return real_unlinkat(dirfd, pathname, flags);
}

int remove(const char *pathname) {
    init_symbols();
    if (is_immutable_target(AT_FDCWD, pathname)) {
        return 0; // sure thing buddy
    }
    return real_remove(pathname);
}

int rename(const char *oldpath, const char *newpath) {
    init_symbols();
    if (is_immutable_target(AT_FDCWD, newpath) || is_immutable_target(AT_FDCWD, oldpath)) {
        return 0; // sure thing buddy
    }
    return real_rename(oldpath, newpath);
}

int renameat(int olddirfd, const char *oldpath, int newdirfd, const char *newpath) {
    init_symbols();
    if (is_immutable_target(newdirfd, newpath) || is_immutable_target(olddirfd, oldpath)) {
        return 0; // sure thing buddy
    }
    return real_renameat(olddirfd, oldpath, newdirfd, newpath);
}

int renameat2(int olddirfd, const char *oldpath, int newdirfd, const char *newpath, unsigned int flags) {
    init_symbols();
    if (is_immutable_target(newdirfd, newpath) || is_immutable_target(olddirfd, oldpath)) {
        return 0; // sure thing buddy
    }
    if (real_renameat2) {
        return real_renameat2(olddirfd, oldpath, newdirfd, newpath, flags);
    }
    return real_renameat(olddirfd, oldpath, newdirfd, newpath);
}

int mkdir(const char *pathname, mode_t mode) {
    init_symbols();
    if (is_immutable_target(AT_FDCWD, pathname)) {
        return 0; // sure thing buddy
    }
    return real_mkdir(pathname, mode);
}

int mkdirat(int dirfd, const char *pathname, mode_t mode) {
    init_symbols();
    if (is_immutable_target(dirfd, pathname)) {
        return 0; // sure thing buddy
    }
    return real_mkdirat(dirfd, pathname, mode);
}

int rmdir(const char *pathname) {
    init_symbols();
    if (is_immutable_target(AT_FDCWD, pathname)) {
        return 0; // sure thing buddy
    }
    return real_rmdir(pathname);
}

int chmod(const char *pathname, mode_t mode) {
    init_symbols();
    if (is_immutable_target(AT_FDCWD, pathname)) {
        return 0; // sure thing buddy
    }
    return real_chmod(pathname, mode);
}

int fchmodat(int dirfd, const char *pathname, mode_t mode, int flags) {
    init_symbols();
    if (is_immutable_target(dirfd, pathname)) {
        return 0; // sure thing buddy
    }
    return real_fchmodat(dirfd, pathname, mode, flags);
}

__attribute__((constructor)) static void nite_sandbox_init(void) {
    init_symbols();
}
