// SPDX-License-Identifier: GPL-3.0-only
#include "shadowhook.h"
#include <sys/system_properties.h>
#include <android/log.h>
#include <atomic>
#include <cstring>
#include <dlfcn.h>
#include <map>
#include <string>

namespace duck::language {
class Properties {
    using Callback = void (*)(void *, const char *, const char *, uint32_t);
    using ReadCallback = void (*)(const prop_info *, Callback, void *);
    using Get = int (*)(const char *, char *);
    using Read = int (*)(const prop_info *, char *, char *);
    ReadCallback original_callback = nullptr;
    Get original_get = nullptr;
    Read original_read = nullptr;
    std::map<std::string, std::string, std::less<>> values;
    struct Call { Properties *owner; Callback callback; void *cookie; };
    // bionic callbacks have no hook context; this immutable owner lives until exit.
    static std::atomic<Properties *> installed;
    static void replace(void *opaque, const char *name, const char *value, uint32_t serial) {
        auto &call = *static_cast<Call *>(opaque);
        auto found = name ? call.owner->values.find(name) : call.owner->values.end();
        call.callback(call.cookie, name, found == call.owner->values.end() ? value : found->second.c_str(), serial);
    }
    static void callback(const prop_info *info, Callback receiver, void *cookie) {
        auto *owner = installed.load(std::memory_order_acquire);
        if (!owner) return;
        auto original = __atomic_load_n(&owner->original_callback, __ATOMIC_ACQUIRE);
        if (!original) return;
        if (!info || !receiver) return original(info, receiver, cookie);
        Call call{owner, receiver, cookie};
        original(info, replace, &call);
    }
    static int get(const char *name, char *value) {
        auto *owner = installed.load(std::memory_order_acquire);
        if (!owner) return 0;
        auto original = __atomic_load_n(&owner->original_get, __ATOMIC_ACQUIRE);
        if (!original) return 0;
        auto found = name ? owner->values.find(name) : owner->values.end();
        if (found == owner->values.end() || !value) return original(name, value);
        std::memcpy(value, found->second.c_str(), found->second.size() + 1);
        return static_cast<int>(found->second.size());
    }
    static int read(const prop_info *info, char *name, char *value) {
        auto *owner = installed.load(std::memory_order_acquire);
        if (!owner) return 0;
        auto original = __atomic_load_n(&owner->original_read, __ATOMIC_ACQUIRE);
        if (!original) return 0;
        char temporary[PROP_NAME_MAX]{};
        char *property = name ? name : temporary;
        int length = original(info, property, value);
        auto found = owner->values.find(property);
        if (found == owner->values.end() || !value) return length;
        std::memcpy(value, found->second.c_str(), found->second.size() + 1);
        return static_cast<int>(found->second.size());
    }
public:
    explicit Properties(const std::string &tag) {
        const auto first = tag.find('-');
        const auto last = tag.rfind('-');
        std::string language = tag.substr(0, first);
        std::string region = last == tag.npos ? "" : tag.substr(last + 1);
        if (region.size() != 2 && region.size() != 3) region.clear();
        values = {{"persist.sys.locale", tag}, {"ro.product.locale", tag},
            {"persist.sys.language", language}, {"ro.product.locale.language", language},
            {"persist.sys.country", region}, {"ro.product.locale.region", region}};
    }
    void install() {
        installed.store(this, std::memory_order_release);
        auto attach = [](const char *name, void *replacement, void **original) {
            void *symbol = dlsym(RTLD_DEFAULT, name);
            if (!symbol || !shadowhook_hook_sym_addr(symbol, replacement, original))
                __android_log_print(ANDROID_LOG_WARN, "DuckLanguage", "Native language hook unavailable: %s", name);
        };
        attach("__system_property_get", reinterpret_cast<void *>(get), reinterpret_cast<void **>(&original_get));
        attach("__system_property_read", reinterpret_cast<void *>(read), reinterpret_cast<void **>(&original_read));
        // A pre-existing unique property hook (e.g. PI in its own selected process)
        // is retained if ShadowHook rejects this optional hook as a conflict.
        attach("__system_property_read_callback", reinterpret_cast<void *>(callback), reinterpret_cast<void **>(&original_callback));
    }
};
std::atomic<Properties *> Properties::installed{nullptr};
void install_properties(const std::string &locale) {
    // Hook targets have C ABI and keep this owner for the target process lifetime.
    auto *policy = new Properties(locale);
    policy->install();
}
}
