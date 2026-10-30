// SPDX-License-Identifier: GPL-3.0-only
#include "runtime.hpp"
#include <unistd.h>

namespace duck {
void Engine::initialize(Table *platform, JNIEnv *env, Entry entry, unsigned char identity) {
    host = platform;
    id = identity;
    abi = nullptr;
    called = false;
    unload = true;
    proxy = *host;
    proxy.impl = this;
    proxy.registerModule = capture;
    proxy.connectCompanion = companion;
    proxy.setOption = option;
    proxy.getModuleDir = directory;
    proxy.getFlags = flags;
    if (entry) entry(&proxy, env);
}
bool Engine::capture(Table *table, Abi *module) {
    auto &engine = *static_cast<Engine *>(table->impl);
    if (engine.abi || !module || module->api_version != 4 || !module->impl
        || !module->preAppSpecialize || !module->postAppSpecialize
        || !module->preServerSpecialize || !module->postServerSpecialize) return false;
    engine.abi = module;
    engine.unload = false;
    return true;
}
void Engine::pre(zygisk::AppSpecializeArgs *args) {
    if (!abi) return;
    called = true;
    abi->preAppSpecialize(abi->impl, args);
}
void Engine::post(const zygisk::AppSpecializeArgs *args) {
    if (called && abi) abi->postAppSpecialize(abi->impl, args);
}
int Engine::companion(void *context) {
    auto &engine = *static_cast<Engine *>(context);
    int fd = engine.host->connectCompanion ? engine.host->connectCompanion(engine.host->impl) : -1;
    if (fd >= 0) {
        auto identity = engine.id;
        if (!channel(fd, identity, true)) { close(fd); return -1; }
    }
    return fd;
}
void Engine::option(void *context, zygisk::Option option) {
    auto &engine = *static_cast<Engine *>(context);
    if (option == zygisk::DLCLOSE_MODULE_LIBRARY) engine.unload = true;
    else if (engine.host->setOption) engine.host->setOption(engine.host->impl, option);
}
int Engine::directory(void *context) {
    auto &engine = *static_cast<Engine *>(context);
    return engine.host->getModuleDir ? engine.host->getModuleDir(engine.host->impl) : -1;
}
uint32_t Engine::flags(void *context) {
    auto &engine = *static_cast<Engine *>(context);
    return engine.host->getFlags ? engine.host->getFlags(engine.host->impl) : 0;
}
}
