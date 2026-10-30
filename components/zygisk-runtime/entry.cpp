// SPDX-License-Identifier: GPL-3.0-only
#include "runtime.hpp"
#include <fcntl.h>
#include <unistd.h>

extern "C" void duck_pif_module_entry(duck::Table *, JNIEnv *);
extern "C" void duck_pif_companion(int);
#ifdef DUCK_HAS_DEVICE_FAKER
extern "C" void duck_faker_module_entry(duck::Table *, JNIEnv *);
// The Rust companion owns a duplicate; this entry owns and closes the incoming fd.
extern "C" void duck_faker_companion(int);
#endif

namespace {
class Runtime : public zygisk::ModuleBase {
public:
    duck::Table *host = nullptr;
    duck::Engine integrity, faker;
    void initialize(duck::Table *platform, JNIEnv *env) {
        host = platform;
        integrity.initialize(platform, env, duck_pif_module_entry, 0);
#ifdef DUCK_HAS_DEVICE_FAKER
        faker.initialize(platform, env, duck_faker_module_entry, 1);
#endif
    }
    void preAppSpecialize(zygisk::AppSpecializeArgs *args) override {
        // PIF claims its process only when its own config/gates keep its hooks resident.
        integrity.pre(args);
        if (integrity.unload) faker.pre(args);
    }
    void postAppSpecialize(const zygisk::AppSpecializeArgs *args) override {
        integrity.post(args);
        faker.post(args);
        release();
    }
    void preServerSpecialize(zygisk::ServerSpecializeArgs *args) override {
        for (auto *engine : {&integrity, &faker}) {
            if (engine->abi) engine->abi->preServerSpecialize(engine->abi->impl, args);
        }
    }
    void postServerSpecialize(const zygisk::ServerSpecializeArgs *args) override {
        for (auto *engine : {&integrity, &faker}) {
            if (engine->abi) engine->abi->postServerSpecialize(engine->abi->impl, args);
        }
        release();
    }
private:
    void release() {
        // A child can never unload code while the other child still needs it.
        if (integrity.unload && faker.unload && host->setOption)
            host->setOption(host->impl, zygisk::DLCLOSE_MODULE_LIBRARY);
    }
};
}
extern "C" [[gnu::visibility("default")]]
void zygisk_module_entry(duck::Table *table, JNIEnv *env) {
    if (!table || !table->registerModule) return;
    static Runtime runtime;
    static duck::Abi abi(&runtime);
    if (!table->registerModule(table, &abi)) return;
    runtime.initialize(table, env);
}
extern "C" [[gnu::visibility("default")]]
void zygisk_companion_entry(int fd) {
    struct Close { int fd; ~Close() { close(fd); } } owned_client{fd};
    unsigned char identity = 255;
    if (!duck::channel(fd, identity, false)) return;
    if (identity == 0) duck_pif_companion(fd);
#ifdef DUCK_HAS_DEVICE_FAKER
    else if (identity == 1) {
        int owned = fcntl(fd, F_DUPFD_CLOEXEC, 0);
        if (owned >= 0) duck_faker_companion(owned);
    }
#endif
}
