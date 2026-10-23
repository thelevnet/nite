// SPDX-License-Identifier: AGPL-3.0-or-later
#include <fcntl.h>
#include <unistd.h>
#include <sys/wait.h>
#include <cstring>
#include <string>
#include <vector>
#include <iostream>

static std::vector<std::string> get_cmdline() {
    std::vector<std::string> args;
    int fd = open("/proc/self/cmdline", O_RDONLY);
    if (fd < 0) return args;

    char buf[4096];
    ssize_t n = read(fd, buf, sizeof(buf) - 1);
    close(fd);
    if (n <= 0) return args;

    buf[n] = '\0';
    ssize_t start = 0;
    for (ssize_t i = 0; i < n; ++i) {
        if (buf[i] == '\0') {
            if (i > start) {
                args.emplace_back(buf + start);
            }
            start = i + 1;
        }
    }
    return args;
}

static bool is_flake_update(const std::vector<std::string>& args) {
    bool has_flake = false;
    bool has_update = false;
    for (const auto& a : args) {
        if (a == "flake") has_flake = true;
        if (a == "update" || a == "lock") has_update = true;
    }
    return has_flake && has_update;
}

static void run_nite_lock() {
    if (access("flake.nix", F_OK) != 0 && access("nite.lock", F_OK) != 0) {
        return;
    }

    std::cout << "[nite] nix flake update detected: refreshing nite.lock..." << std::endl;
    pid_t pid = fork();
    if (pid == 0) {
        execlp("nite", "nite", "lock", nullptr);
        execlp("nite-lock", "nite-lock", nullptr);
        _exit(127);
    } else if (pid > 0) {
        int status = 0;
        waitpid(pid, &status, 0);
    }
}

__attribute__((constructor)) static void init_nix_nite() {
    auto args = get_cmdline();
    if (is_flake_update(args)) {
        run_nite_lock();
    }
}
