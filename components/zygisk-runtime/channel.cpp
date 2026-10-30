// SPDX-License-Identifier: GPL-3.0-only
#include "runtime.hpp"
#include <algorithm>
#include <cerrno>
#include <chrono>
#include <poll.h>
#include <sys/socket.h>

namespace duck {
bool channel(int fd, unsigned char &id, bool writing) {
    std::array<unsigned char, 5> frame{prefix[0], prefix[1], prefix[2], prefix[3], id};
    const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(1);
    size_t offset = 0;
    while (offset < frame.size()) {
        const auto remaining = std::chrono::duration_cast<std::chrono::milliseconds>(deadline - std::chrono::steady_clock::now()).count();
        if (remaining <= 0) return false;
        pollfd event{fd, static_cast<short>(writing ? POLLOUT : POLLIN), 0};
        int ready = poll(&event, 1, static_cast<int>(remaining));
        if (ready < 0 && errno == EINTR) continue;
        if (ready <= 0 || (event.revents & (POLLERR | POLLNVAL))) return false;
        const auto count = writing
            ? send(fd, frame.data() + offset, frame.size() - offset, MSG_NOSIGNAL | MSG_DONTWAIT)
            : recv(fd, frame.data() + offset, frame.size() - offset, MSG_DONTWAIT);
        if (count < 0 && (errno == EINTR || errno == EAGAIN || errno == EWOULDBLOCK)) continue;
        if (count <= 0) return false;
        offset += static_cast<size_t>(count);
    }
    if (!std::equal(prefix.begin(), prefix.end(), frame.begin()) || frame.back() > 1) return false;
    id = frame.back();
    return true;
}
}
