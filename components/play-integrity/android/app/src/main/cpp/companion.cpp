// SPDX-License-Identifier: GPL-3.0-only
// Derived from Play Integrity Fork companion, revision 23ec56c.
// Modified 2026-10-24: checked complete IO, fixed-width lengths, bounded files,
// lifecycle options and a shared monotonic exchange deadline; payload receipt is not injection proof.
#include "transport.hpp"
#include <cerrno>
#include <cstdio>
#include <fstream>
#include <fcntl.h>
#include <poll.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <unistd.h>
#ifndef PIF_MODULE_DIR
#define PIF_MODULE_DIR "/data/adb/modules/duck-toolbox/play-integrity"
#endif
#ifndef PIF_DATA_DIR
#define PIF_DATA_DIR "/data/adb/play-integrity"
#endif
namespace pif {
bool transfer(int fd, void *buffer, size_t size, bool writing, Deadline deadline) {
    const int flags = fcntl(fd, F_GETFL);
    if (flags < 0 || fcntl(fd, F_SETFL, flags | O_NONBLOCK) != 0) return false;
    struct RestoreFlags {
        int fd, flags;
        ~RestoreFlags() { fcntl(fd, F_SETFL, flags); }
    } restore{fd, flags};
    auto *bytes = static_cast<char *>(buffer);
    while (size) {
        const auto remaining = deadline - std::chrono::steady_clock::now();
        if (remaining <= Deadline::duration::zero()) return false;
        pollfd socket{fd, static_cast<short>(writing ? POLLOUT : POLLIN), 0};
        const auto milliseconds = std::chrono::duration_cast<std::chrono::milliseconds>(remaining).count() + 1;
        const int ready = poll(&socket, 1, static_cast<int>(milliseconds));
        if (ready < 0 && errno == EINTR) continue;
        if (ready <= 0 || (socket.revents & (POLLNVAL | POLLERR))
            || std::chrono::steady_clock::now() >= deadline) return false;
        auto count = writing ? send(fd, bytes, size, MSG_NOSIGNAL | MSG_DONTWAIT)
                             : recv(fd, bytes, size, MSG_DONTWAIT);
        if (count < 0 && (errno == EINTR || errno == EAGAIN || errno == EWOULDBLOCK)) continue;
        if (count <= 0) return false;
        bytes += count; size -= static_cast<size_t>(count);
    }
    return true;
}
static std::vector<char> file(const std::string &path, size_t limit) {
    std::ifstream stream(path, std::ios::binary | std::ios::ate);
    if (!stream) return {};
    auto length = stream.tellg();
    if (length <= 0 || static_cast<uint64_t>(length) > limit) return {};
    std::vector<char> bytes(static_cast<size_t>(length));
    stream.seekg(0);
    if (!stream.read(bytes.data(), static_cast<std::streamsize>(bytes.size()))) return {};
    return bytes;
}
static bool active() {
    if (access(PIF_DATA_DIR "/disabled", F_OK) == 0) {
        struct stat metadata{};
        if (stat(PIF_DATA_DIR "/disabled", &metadata) != 0 || metadata.st_size <= 0 || metadata.st_size > 128
            || stat(PIF_DATA_DIR "/enabled-token", &metadata) != 0 || metadata.st_size > 128
            || file(PIF_DATA_DIR "/disabled",128) != file(PIF_DATA_DIR "/enabled-token",128)) return false;
    }
    if (access(PIF_MODULE_DIR "/../disable", F_OK) == 0
        || access(PIF_MODULE_DIR "/../remove", F_OK) == 0) return false;
    for (const auto *id : {"playintegrityfix", "safetynet-fix"}) {
        auto path = std::string("/data/adb/modules/") + id;
        if (access((path + "/module.prop").c_str(), F_OK) == 0 && access((path + "/disable").c_str(), F_OK) != 0 && access((path + "/remove").c_str(), F_OK) != 0) return false;
    }
    const auto bytes = file(PIF_DATA_DIR "/options.json", max_config);
    if (bytes.empty()) return false;
    const auto opts = nlohmann::json::parse(bytes.begin(), bytes.end(), nullptr, false);
    if (!opts.is_object()) return false;
    for (const auto *key : {"skipdelprop", "skippersistprop", "skipmatchdevice"}) {
        if (opts.contains(key) && !opts[key].is_boolean()) return false;
    }
    if (!opts.contains("enabled") || !opts["enabled"].is_boolean() || !opts["enabled"].get<bool>()) return false;
    return !opts.contains("mode") || opts["mode"] == "full";
}
bool receive(int fd, std::vector<char> &dex, std::string &config) {
    dex.clear(); config.clear();
    const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(2);
    uint32_t header[3]{};
    if (!transfer(fd, header, sizeof(header), false, deadline) || header[0] != magic || !header[1] || header[1] > max_dex || !header[2] || header[2] > max_config) return false;
    dex.resize(header[1]); config.resize(header[2]);
    if (!transfer(fd, dex.data(), dex.size(), false, deadline) || !transfer(fd, config.data(), config.size(), false, deadline)) { dex.clear(); config.clear(); return false; }
    return true;
}
void companion(int fd) {
    const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(2);
    auto dex = active() ? file(PIF_MODULE_DIR "/classes.dex", max_dex) : std::vector<char>{};
    auto config = dex.empty() ? std::vector<char>{} : file(PIF_DATA_DIR "/custom.pif.json", max_config);
    if (!active()) { dex.clear(); config.clear(); }
    uint32_t header[]{magic, static_cast<uint32_t>(dex.size()), static_cast<uint32_t>(config.size())};
    if (!transfer(fd, header, sizeof(header), true, deadline)) return;
    if (!dex.empty() && !transfer(fd, dex.data(), dex.size(), true, deadline)) return;
    if (!config.empty()) transfer(fd, config.data(), config.size(), true, deadline);
}
}
