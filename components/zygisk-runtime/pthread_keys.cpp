// SPDX-License-Identifier: GPL-3.0-only
#include "pthread_keys.hpp"
#include <atomic>
#include <cerrno>
#include <limits.h>
#include <pthread.h>

using Destructor = void (*)(void *);

extern "C" int __real_pthread_key_create(pthread_key_t *, Destructor);
extern "C" int __real_pthread_key_delete(pthread_key_t);

namespace {
struct Key {
    pthread_key_t id;
    Destructor destroy;
    bool active;
};

// Constructors and unloadable specialization run on the loader/callback thread.
// Companion workers and persistent hooks use libc directly after the transition.
Key keys[PTHREAD_KEYS_MAX]{};
constinit std::atomic<bool> resident{false};
}

void duck::keep_pthread_keys() {
    resident.store(true, std::memory_order_release);
}

bool duck::pthread_keys_allow_unload() {
    return !resident.load(std::memory_order_acquire);
}

extern "C" int __wrap_pthread_key_create(pthread_key_t *id, Destructor destroy) {
    if (!duck::pthread_keys_allow_unload()) return __real_pthread_key_create(id, destroy);
    for (auto &key : keys) {
        if (key.active) continue;
        // Bionic invalidates a deleted key without clearing its destructor field.
        // Keep the callback address in this DSO until the finalizer has run it.
        int result = __real_pthread_key_create(id, nullptr);
        if (result == 0) key = {*id, destroy, true};
        return result;
    }
    return EAGAIN;
}

extern "C" int __wrap_pthread_key_delete(pthread_key_t id) {
    if (!duck::pthread_keys_allow_unload()) return __real_pthread_key_delete(id);
    for (auto &key : keys) {
        if (!key.active || key.id != id) continue;
        int result = __real_pthread_key_delete(id);
        if (result == 0) key = {};
        return result;
    }
    return EINVAL;
}

// Run after C++ static destructors and before Android's .fini_array.0 emutls
// cleanup. The latter may try to delete an already released key via the wrapper.
[[gnu::destructor(101)]] static void release_pthread_keys() {
    if (!duck::pthread_keys_allow_unload()) return;
    for (int round = 0; round < PTHREAD_DESTRUCTOR_ITERATIONS; ++round) {
        bool called = false;
        for (auto key : keys) {
            if (!key.active || !key.destroy) continue;
            void *value = pthread_getspecific(key.id);
            if (value && pthread_setspecific(key.id, nullptr) == 0) {
                key.destroy(value);
                called = true;
            }
        }
        if (!called) break;
    }
    for (auto &key : keys) {
        if (key.active) __wrap_pthread_key_delete(key.id);
    }
}
