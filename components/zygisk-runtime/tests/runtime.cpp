// SPDX-License-Identifier: GPL-3.0-only
#include "runtime.hpp"
#include <cassert>
#include <chrono>
#include <fcntl.h>
#include <sys/socket.h>
#include <unistd.h>

namespace {
duck::Abi *registered = nullptr;
unsigned unloads = 0, forces = 0, faker_calls = 0;
bool pif_keeps = false, faker_keeps = false;
int socket_fd = -1;
bool register_root(duck::Table *, duck::Abi *abi) { registered = abi; return abi->api_version == 4; }
void host_option(void *, zygisk::Option option) {
    if (option == zygisk::DLCLOSE_MODULE_LIBRARY) ++unloads;
    else if (option == zygisk::FORCE_DENYLIST_UNMOUNT) ++forces;
}
int host_connect(void *) { return socket_fd; }
int host_directory(void *) { return 37; }
uint32_t host_flags(void *) { return 19; }
class Probe : public zygisk::ModuleBase {
public:
    duck::Table *table = nullptr;
    bool faker = false;
    void preAppSpecialize(zygisk::AppSpecializeArgs *) override {
        if (faker) ++faker_calls;
        table->setOption(table->impl, zygisk::FORCE_DENYLIST_UNMOUNT);
        if (!(faker ? faker_keeps : pif_keeps)) table->setOption(table->impl, zygisk::DLCLOSE_MODULE_LIBRARY);
    }
    void preServerSpecialize(zygisk::ServerSpecializeArgs *) override {
        table->setOption(table->impl, zygisk::DLCLOSE_MODULE_LIBRARY);
    }
};
Probe pif_probe, faker_probe;
duck::Abi pif_abi(&pif_probe), faker_abi(&faker_probe);
void specialize(bool keep_pif, bool keep_faker) {
    pif_keeps = keep_pif; faker_keeps = keep_faker;
    unloads = forces = faker_calls = 0;
    duck::Table host{};
    host.registerModule = register_root;
    host.setOption = host_option;
    host.connectCompanion = host_connect;
    host.getModuleDir = host_directory;
    host.getFlags = host_flags;
    zygisk_module_entry(&host, nullptr);
    assert(registered);
    registered->preAppSpecialize(registered->impl, nullptr);
    assert(unloads == 0); // A child cannot unload the shared ELF from inside a callback.
    registered->postAppSpecialize(registered->impl, nullptr);
    assert(faker_calls == (keep_pif ? 0u : 1u));
    assert(unloads == ((!keep_pif && !keep_faker) ? 1u : 0u));
    assert(pif_probe.table->getModuleDir(pif_probe.table->impl) == 37);
    assert(faker_probe.table->getFlags(faker_probe.table->impl) == 19);
    registered->preServerSpecialize(registered->impl, nullptr);
    unsigned before = unloads;
    registered->postServerSpecialize(registered->impl, nullptr);
    assert(unloads == before + 1);
}
}
extern "C" void duck_pif_module_entry(duck::Table *table, JNIEnv *) {
    pif_probe.table = table;
    assert(table->registerModule(table, &pif_abi));
}
extern "C" void duck_faker_module_entry(duck::Table *table, JNIEnv *) {
    faker_probe.table = table; faker_probe.faker = true;
    assert(table->registerModule(table, &faker_abi));
}
extern "C" void duck_pif_companion(int fd) { assert(write(fd, "P", 1) == 1); }
extern "C" void duck_faker_companion(int fd) { assert(write(fd, "F", 1) == 1); close(fd); }
int main() {
    static_assert(ZYGISK_API_VERSION == 4);
    static_assert(sizeof(duck::Table) == 10 * sizeof(void *));
    for (bool pif : {false,true}) for (bool faker : {false,true}) specialize(pif, faker);
    for (unsigned char identity : {0,1}) {
        int pair[2]; assert(socketpair(AF_UNIX, SOCK_STREAM, 0, pair) == 0);
        assert(duck::channel(pair[0], identity, true));
        zygisk_companion_entry(pair[1]);
        char response = 0; assert(read(pair[0], &response, 1) == 1);
        assert(response == (identity == 0 ? 'P' : 'F'));
        assert(fcntl(pair[1], F_GETFD) < 0); // Root entry closes the incoming descriptor once.
        close(pair[0]);
    }
    int pair[2]; assert(socketpair(AF_UNIX, SOCK_STREAM, 0, pair) == 0);
    auto start = std::chrono::steady_clock::now(); unsigned char identity = 0;
    assert(!duck::channel(pair[0], identity, false));
    assert(std::chrono::steady_clock::now() - start < std::chrono::seconds(2));
    close(pair[0]); close(pair[1]);
}
