// SPDX-License-Identifier: GPL-3.0-only
#pragma once
#include <algorithm>
#include <cerrno>
#include <chrono>
#include <cstdint>
#include <poll.h>
#include <sys/socket.h>
#include <vector>

namespace duck::language {
class Transport {
    int fd;
    std::chrono::steady_clock::time_point deadline = std::chrono::steady_clock::now() + std::chrono::seconds(2);
public:
    explicit Transport(int descriptor) : fd(descriptor) {}
    bool bytes(void *data, size_t length, bool writing) {
        auto *cursor = static_cast<char *>(data);
        while (length) {
            auto left = std::chrono::duration_cast<std::chrono::milliseconds>(deadline - std::chrono::steady_clock::now()).count();
            if (left <= 0) return false;
            pollfd event{fd, static_cast<short>(writing ? POLLOUT : POLLIN), 0};
            int ready = poll(&event, 1, static_cast<int>(left));
            if (ready < 0 && errno == EINTR) continue;
            if (ready <= 0 || event.revents & (POLLERR | POLLNVAL)) return false;
            auto count = writing ? send(fd, cursor, length, MSG_NOSIGNAL | MSG_DONTWAIT) : recv(fd, cursor, length, MSG_DONTWAIT);
            if (count < 0 && (errno == EAGAIN || errno == EWOULDBLOCK || errno == EINTR)) continue;
            if (count <= 0) return false;
            cursor += count; length -= static_cast<size_t>(count);
        }
        return true;
    }
    bool number(uint32_t &number, bool writing) { return bytes(&number, sizeof(number), writing); }
    template<class T> bool blob(T &value, size_t maximum, bool writing) {
        if (writing && (value.size() > maximum || value.size() > UINT32_MAX)) return false;
        uint32_t size = writing ? static_cast<uint32_t>(value.size()) : 0;
        if (!number(size, writing) || size > maximum) return false;
        if (!writing) value.resize(size);
        return bytes(value.data(), size, writing);
    }
};
}
