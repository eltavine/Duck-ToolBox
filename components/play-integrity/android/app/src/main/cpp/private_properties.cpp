// SPDX-License-Identifier: GPL-3.0-only
#include "private_properties.hpp"
#include <algorithm>
#include <sys/system_properties.h>

#ifdef DUCK_HAS_DEVICE_FAKER
struct Property { const char *name; const char *value; };
extern "C" bool duck_faker_patch_existing(const Property *entries, size_t count);
#endif

namespace pif {
static bool matchesDirect(const char *name, const std::string &value) {
    // Bionic exposes long values through the callback API only.
    if (value.size() >= PROP_VALUE_MAX) return true;
    char direct[PROP_VALUE_MAX]{};
    return __system_property_get(name, direct) > 0 && value == direct;
}

bool PrivateProperties::prepare(const Settings &settings) {
#ifdef DUCK_HAS_DEVICE_FAKER
    std::vector<Property> pending;
    struct Scan {
        const Settings &settings;
        PropertyValues &found;
        std::vector<Property> &pending;

        static void record(void *cookie, const char *name, const char *current, uint32_t) {
            auto &scan = *static_cast<Scan *>(cookie);
            if (!name || scan.found.size() > 1024) return;
            auto value = propertyOverride(scan.settings.properties, name);
            if (!value) {
                const auto fallback = scan.settings.privateDefaults.find(name);
                if (fallback == scan.settings.privateDefaults.end()) return;
                value = &fallback->second;
            }
            auto [entry, inserted] = scan.found.emplace(name, *value);
            if (inserted && (!current || *value != current || !matchesDirect(name, *value))) {
                scan.pending.push_back({entry->first.c_str(), entry->second.c_str()});
            }
        }

        void find(const std::string &key) {
            if (!found.contains(key)) {
                if (const auto *property = __system_property_find(key.c_str())) {
                    __system_property_read_callback(property, record, this);
                }
            }
        }
    } scan{settings, expected, pending};
    int result = 0;
    if (std::ranges::any_of(settings.properties, [](const auto &rule) { return rule.first.starts_with('*'); })) {
        result = __system_property_foreach([](const prop_info *property, void *cookie) {
            __system_property_read_callback(property, Scan::record, cookie);
        }, &scan);
    } else {
        for (const auto &rule : settings.properties) scan.find(rule.first);
        for (const auto &rule : settings.privateDefaults) scan.find(rule.first);
    }
    if (result == 0 && !expected.empty() && expected.size() <= 1024) {
        if (pending.empty() || duck_faker_patch_existing(pending.data(), pending.size())) return true;
    }
    expected.clear();
#else
    (void)settings;
#endif
    return false;
}

size_t PrivateProperties::matching() const {
    size_t matched = 0;
    for (const auto &[key, value] : expected) {
        const auto *property = __system_property_find(key.c_str());
        if (!property) continue;
        struct Readback { const std::string &expected; bool matches = false; } readback{value};
        __system_property_read_callback(property, [](void *cookie, const char *, const char *text, uint32_t) {
            auto &result = *static_cast<Readback *>(cookie);
            result.matches = text && result.expected == text;
        }, &readback);
        if (readback.matches && matchesDirect(key.c_str(), value)) ++matched;
    }
    return matched;
}
}
