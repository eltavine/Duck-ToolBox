// SPDX-License-Identifier: GPL-3.0-only
#include <sys/system_properties.h>
#include <cassert>
#include <cstdarg>
#include <cstring>
#include <string>

namespace duck::language { void install_properties(const std::string &); }
using Callback = void (*)(void *, const char *, const char *, uint32_t);
using Get = int (*)(const char *, char *);
using Read = int (*)(const prop_info *, char *, char *);
using ReadCallback = void (*)(const prop_info *, Callback, void *);
static int original_get(const char *, char *value) { std::strcpy(value, "original"); return 8; }
static int original_read(const prop_info *info, char *name, char *value) {
    if (name) std::strcpy(name, info->name);
    std::strcpy(value, info->value); return static_cast<int>(std::strlen(value));
}
static void original_callback(const prop_info *info, Callback callback, void *cookie) {
    callback(cookie, info->name, info->value, 42);
}
static Get get = original_get;
static Read read = original_read;
static ReadCallback callback = original_callback;
extern "C" void *duck_test_dlsym(void *, const char *name) {
    if (!std::strcmp(name, "__system_property_get")) return reinterpret_cast<void *>(original_get);
    if (!std::strcmp(name, "__system_property_read")) return reinterpret_cast<void *>(original_read);
    if (!std::strcmp(name, "__system_property_read_callback")) return reinterpret_cast<void *>(original_callback);
    return nullptr;
}
extern "C" void *shadowhook_hook_sym_addr(void *target, void *replacement, void **original) {
    __atomic_store_n(original, target, __ATOMIC_RELEASE);
    if (target == reinterpret_cast<void *>(original_get)) get = reinterpret_cast<Get>(replacement);
    if (target == reinterpret_cast<void *>(original_read)) read = reinterpret_cast<Read>(replacement);
    if (target == reinterpret_cast<void *>(original_callback)) callback = reinterpret_cast<ReadCallback>(replacement);
    if (target == reinterpret_cast<void *>(original_get)) {
        char value[PROP_VALUE_MAX]{};
        assert(get("unrelated", value) == 8 && std::string(value) == "original");
    }
    return target;
}
int __android_log_print(int, const char *, const char *, ...) { return 0; }
int main() {
    for (const char *tag : {"en-US", "zh-Hant-TW", "ar"}) {
        duck::language::install_properties(tag);
        char value[PROP_VALUE_MAX]{}, name[PROP_NAME_MAX]{};
        assert(get("persist.sys.locale", value) == static_cast<int>(std::strlen(tag)));
        assert(std::string(value) == tag);
        assert(get("unrelated", value) == 8 && std::string(value) == "original");
        prop_info property{"persist.sys.locale", "real"};
        assert(read(&property, name, value) == static_cast<int>(std::strlen(tag)));
        assert(std::string(value) == tag && std::string(name) == property.name);
        assert(read(&property, nullptr, value) == static_cast<int>(std::strlen(tag)));
        assert(std::string(value) == tag);
        std::string result;
        callback(&property, [](void *cookie, const char *name, const char *value, uint32_t serial) {
            assert(std::string(name) == "persist.sys.locale" && serial == 42);
            *static_cast<std::string *>(cookie) = value;
        }, &result);
        assert(result == tag);
        prop_info other{"unrelated", "unchanged"};
        callback(&other, [](void *cookie, const char *, const char *value, uint32_t serial) {
            assert(serial == 42); *static_cast<std::string *>(cookie) = value;
        }, &result);
        assert(result == "unchanged");
    }
}
