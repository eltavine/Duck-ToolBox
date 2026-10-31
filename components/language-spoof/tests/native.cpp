// SPDX-License-Identifier: GPL-3.0-only
#include "language.hpp"
#include "transport.hpp"
#include <cassert>
#include <filesystem>
#include <fstream>
#include <sys/socket.h>
#include <thread>
#include <unistd.h>

using namespace duck::language;
static void put(const std::string &name, const std::string &text) {
    std::filesystem::create_directories(std::filesystem::path(name).parent_path());
    std::ofstream(name) << text;
}
int main(int argc, char **argv) {
    assert(argc == 2);
    std::string root = argv[1];
    Target main, work;
    assert(target(10123, "com.example.app:remote", "/data/user/0/com.example.app", main));
    assert(target(1010123, "com.example.app", "/data/user_de/10/com.example.app", work));
    Target other;
    assert(target(10123, "com.example.app", "/data/data/com.example.app", other));
    assert(target(1010123, "com.example.app", "/mnt/expand/volume/user/10/com.example.app", other));
    assert(!target(1000, "com.example.app", "/data/user/0/com.example.app", other));
    assert(!target(99001, "com.example.app", "/data/user/0/com.example.app", other));
    assert(!target(1010123, "com.example.app", "/data/user/0/com.example.app", other));
    assert(!target(10123, "com.other.app", "/data/user/0/com.example.app", other));
    assert(!target(10123, "com.example.app.other", "/data/user/0/com.example.app", other));
    assert(!target(10123, "com.example.app", "/untrusted/0/com.example.app", other));
    assert(!target(10123, "com.example.app:", "/data/user/0/com.example.app", other));
    assert(!target(10123, "com.example.app", "/mnt/expand/volume/nested/user/0/com.example.app", other));
    assert(!target(10123, "com.example.app", "/mnt/expand//user/0/com.example.app", other));
    for (const char *invalid : {"1", "1:", "1:\n", "1:a/b", "0:token"}) assert(!active(invalid));
    assert(active("1:.tmp-A_123"));
    std::string rules = R"({"rules":[{"package_name":"com.example.app","user_id":0,"locale":"en-US"},{"package_name":"com.example.app","user_id":10,"locale":"ja-JP"}]})";
    assert(select(rules, main) == "en-US");
    assert(select(rules, work) == "ja-JP");
    assert(select("corrupt", main).empty());
    assert(select(R"({"rules":[],"rules":[]})", main).empty());
    assert(select(R"({"rules":[{"package_name":"com.example.app","user_id":0,"locale":"en","locale":"ja"}]})", main).empty());
    assert(select(R"({"rules":[{"package_name":"com.example.app","user_id":0,"locale":"EN-us"}]})", main) == "en-US");
    assert(select(R"({"rules":[{"package_name":"com.example.app","user_id":0,"locale":"UND"}]})", main).empty());
    assert(select(R"({"rules":[{"package_name":"com.example.app","user_id":0,"locale":"en"},{"package_name":"com.other.app","user_id":0,"locale":"en"},{"package_name":"com.other.app","user_id":0,"locale":"ja"}]})", main).empty());
    for (const char *tag : {"en_US", "en--US", "en-US-x-private", "a", "en-1234", "en;id"}) {
        auto malformed = std::string(R"({"rules":[{"package_name":"com.example.app","user_id":0,"locale":")") + tag + R"("}]})";
        assert(select(malformed, main).empty());
    }
    put(root + "/data/rules.json", rules);
    put(root + "/data/enabled", "1:activation");
    put(root + "/module/play-integrity/classes.dex", "test-dex");
    auto request = [&](const Target &target) {
        int fds[2]; assert(socketpair(AF_UNIX, SOCK_STREAM, 0, fds) == 0);
        std::thread server([&] { companion(fds[1]); close(fds[1]); });
        std::string locale; std::vector<char> dex;
        bool result = exchange(fds[0], target, locale, dex);
        close(fds[0]); server.join();
        if (result) assert(std::string(dex.begin(), dex.end()) == "test-dex");
        return result ? locale : "";
    };
    assert(request(main) == "en-US");
    assert(request(work) == "ja-JP");
    put(root + "/module/disable", "");
    assert(request(main).empty());
    std::filesystem::remove(root + "/module/disable");
    std::filesystem::create_symlink(root + "/missing", root + "/module/disable");
    assert(request(main).empty());
    std::filesystem::remove(root + "/module/disable");
    put(root + "/data/enabled", "0:stopped");
    assert(request(main).empty());
    put(root + "/data/enabled", "1:activation");
    put(root + "/data/rules.json", "broken");
    assert(request(main).empty());
    put(root + "/data/rules.json", rules);
    std::filesystem::remove(root + "/data/rules.json");
    std::filesystem::create_symlink(root + "/private.json", root + "/data/rules.json");
    put(root + "/private.json", rules);
    assert(request(main).empty());
    int fds[2]; assert(socketpair(AF_UNIX, SOCK_STREAM, 0, fds) == 0);
    Transport transport(fds[0]);
    auto start = std::chrono::steady_clock::now();
    uint32_t number = 0;
    assert(!transport.number(number, false));
    assert(std::chrono::steady_clock::now() - start < std::chrono::seconds(3));
    close(fds[0]); close(fds[1]);
    assert(socketpair(AF_UNIX, SOCK_STREAM, 0, fds) == 0);
    Transport output(fds[0]);
    std::string oversized(256, 'x');
    assert(!output.blob(oversized, 255, true));
    char byte;
    assert(recv(fds[1], &byte, 1, MSG_DONTWAIT) < 0 && (errno == EAGAIN || errno == EWOULDBLOCK));
    close(fds[0]); close(fds[1]);
}
