// SPDX-License-Identifier: GPL-3.0-only
#include <cassert>
#include <pthread.h>

#ifdef DUCK_KEY_FIXTURE
#include "pthread_keys.hpp"

// Darwin ld has no --wrap. ELF builds exercise the production linker flags.
#ifdef __APPLE__
extern "C" int __real_pthread_key_create(pthread_key_t *key, void (*destroy)(void *)) {
    return pthread_key_create(key, destroy);
}
extern "C" int __real_pthread_key_delete(pthread_key_t key) {
    return pthread_key_delete(key);
}
extern "C" int __wrap_pthread_key_create(pthread_key_t *, void (*)(void *));
extern "C" int __wrap_pthread_key_delete(pthread_key_t);
#define pthread_key_create __wrap_pthread_key_create
#define pthread_key_delete __wrap_pthread_key_delete
#endif

namespace {
pthread_key_t temporary, no_destructor;
void destroy_temporary(void *value) {
    assert(pthread_getspecific(temporary) == nullptr);
    auto *count = static_cast<int *>(value);
    if (++*count == 1) assert(pthread_setspecific(temporary, count) == 0);
}
[[gnu::constructor]] void register_keys() {
    assert(pthread_key_create(&temporary, destroy_temporary) == 0);
    assert(pthread_key_create(&no_destructor, nullptr) == 0);
}
}

extern "C" void prepare_keys(int *count) {
    assert(pthread_setspecific(temporary, count) == 0);
    pthread_key_t deleted;
    assert(pthread_key_create(&deleted, destroy_temporary) == 0);
    assert(pthread_setspecific(deleted, count) == 0);
    assert(pthread_key_delete(deleted) == 0);
}

extern "C" void *exercise_resident_key(void *value) {
    duck::keep_pthread_keys();
    pthread_key_t resident;
    assert(pthread_key_create(&resident, [](void *count) { ++*static_cast<int *>(count); }) == 0);
    assert(pthread_setspecific(resident, value) == 0);
    return nullptr;
}

#else
#include <cerrno>
#include <cstdio>
#include <dlfcn.h>
#include <vector>

static void *load(const char *path) {
    void *library = dlopen(path, RTLD_LOCAL | RTLD_NOW);
    if (!library) std::fprintf(stderr, "%s\n", dlerror());
    assert(library);
    return library;
}

static size_t available_keys() {
    std::vector<pthread_key_t> keys;
    pthread_key_t key;
    int result;
    while ((result = pthread_key_create(&key, nullptr)) == 0) keys.push_back(key);
    assert(result == EAGAIN);
    for (auto id : keys) assert(pthread_key_delete(id) == 0);
    return keys.size();
}

int main(int argc, char **argv) {
    assert(argc == 2);
    pthread_key_t unrelated;
    assert(pthread_key_create(&unrelated, nullptr) == 0);
    int destroyed = 0;
    assert(pthread_setspecific(unrelated, &destroyed) == 0);
    const size_t capacity = available_keys();
    for (unsigned pass = 0; pass < 10; ++pass) {
        void *library = load(argv[1]);
        auto prepare = reinterpret_cast<void (*)(int *)>(dlsym(library, "prepare_keys"));
        assert(prepare);
        destroyed = 0;
        prepare(&destroyed);
        assert(destroyed == 0);
        assert(dlclose(library) == 0);
        assert(destroyed == 2);
        assert(pthread_getspecific(unrelated) == &destroyed);
        assert(available_keys() == capacity);
    }
    assert(pthread_key_delete(unrelated) == 0);

    // Process-lifetime consumers must still get regular worker-thread destructors.
    void *library = load(argv[1]);
    auto worker = reinterpret_cast<void *(*)(void *)>(dlsym(library, "exercise_resident_key"));
    assert(worker);
    destroyed = 0;
    pthread_t thread;
    assert(pthread_create(&thread, nullptr, worker, &destroyed) == 0);
    assert(pthread_join(thread, nullptr) == 0);
    assert(destroyed == 1);
}
#endif
