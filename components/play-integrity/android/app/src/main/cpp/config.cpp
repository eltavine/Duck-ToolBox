// SPDX-License-Identifier: GPL-3.0-only
// Derived from Play Integrity Fork native config handling (23ec56c).
// Modified 2026-10-24: scalar normalization, rejected invalid UTF-8 and bounded parsing.
#include "transport.hpp"
#include "settings.hpp"
#include <array>
#include <charconv>
#include <sstream>
namespace pif {
int number(const nlohmann::json &value, int fallback) {
    if (!value.is_string()) return fallback;
    const auto text = value.get<std::string>();
    int result = 0;
    auto parsed = std::from_chars(text.data(), text.data() + text.size(), result);
    return parsed.ec == std::errc{} && parsed.ptr == text.data() + text.size() && result >= 0 && result <= 100 ? result : fallback;
}
nlohmann::json parse(const std::string &raw) {
    if (raw.size() > max_config) return {};
    nlohmann::json result;
    if (nlohmann::json::accept(raw, true)) result = nlohmann::json::parse(raw, nullptr, false, true);
    else {
        result = nlohmann::json::object();
        std::istringstream stream(raw);
        std::string line;
        while (std::getline(stream, line)) {
            const auto first = line.find_first_not_of(" \t\r");
            if (first == std::string::npos || line[first] == '#') continue;
            auto delimiter = line.find('=', first);
            if (delimiter == std::string::npos) return {};
            auto key = line.substr(first, delimiter - first);
            auto end = key.find_last_not_of(" \t\r");
            if (end == std::string::npos) return {};
            key.resize(end + 1);
            auto value = line.substr(delimiter + 1);
            auto comment = value.find('#');
            if (comment != std::string::npos) value.resize(comment);
            auto begin = value.find_first_not_of(" \t\r");
            value = begin == std::string::npos ? "" : value.substr(begin);
            end = value.find_last_not_of(" \t\r");
            if (end != std::string::npos) value.resize(end + 1);
            result[key] = value;
        }
    }
    if (!result.is_object() || result.size() > 512) return {};
    for (auto &entry : result.items()) {
        if (entry.key().empty() || entry.key().size() > 128) return {};
        // Match the standalone CLI's accepted JSON scalar representation.
        if (entry.value().is_boolean()) entry.value() = entry.value().get<bool>() ? "1" : "0";
        else if (entry.value().is_number()) entry.value() = entry.value().dump();
        else if (entry.value().is_null()) entry.value() = "";
        else if (!entry.value().is_string()) return {};
        const auto value = entry.value().get<std::string>();
        if (value.size() > 4096 || value.find_first_of(std::string("\0\n\r", 3)) != std::string::npos) return {};
    }
    // Prop input does not pass through the JSON parser. Reject byte sequences that
    // the JSON serializer would replace, before the exception-free injection build
    // serializes Build fields. Valid strings and unknown fields remain unchanged.
    auto checked = nlohmann::json::parse(result.dump(-1, ' ', false,
        nlohmann::json::error_handler_t::replace), nullptr, false);
    return checked == result ? result : nlohmann::json::object();
}
}

namespace pif {
const std::string *propertyOverride(const PropertyValues &rules, std::string_view name) {
    if (auto exact = rules.find(name); exact != rules.end()) return &exact->second;
    for (const auto &[key, value] : rules) {
        if (key.starts_with('*') && name.ends_with(std::string_view(key).substr(1))) return &value;
    }
    return nullptr;
}

static void expandFingerprint(nlohmann::json &config) {
    static constexpr std::array fields{"BRAND", "PRODUCT", "DEVICE", "RELEASE", "ID", "INCREMENTAL", "TYPE", "TAGS"};
    std::string fingerprint = config.value("FINGERPRINT", std::string{});
    std::array<std::string_view, fields.size()> values;
    size_t start = 0;
    for (size_t i = 0; i < fields.size(); ++i) {
        const auto end = fingerprint.find_first_of("/:", start);
        if (end == start || (end == std::string::npos) != (i == fields.size() - 1)) return;
        values[i] = std::string_view(fingerprint).substr(start, end - start);
        if (values[i].empty()) return;
        start = end + 1;
    }
    for (size_t i = 0; i < fields.size(); ++i) {
        if (config.value(fields[i], std::string{}).empty()) config[fields[i]] = values[i];
    }
}

static void fillPrivateDefaults(Settings &result) {
    result.privateDefaults = {
        {"ro.board.first_api_level", "32"}, {"init.svc.adbd", "stopped"}, {"sys.usb.state", "mtp"},
    };
    for (const auto &[field, property] : std::array{
        std::pair{"FINGERPRINT", "ro.build.fingerprint"}, std::pair{"ID", "ro.build.id"},
        std::pair{"DEVICE", "ro.product.device"}, std::pair{"PRODUCT", "ro.product.name"},
        std::pair{"BRAND", "ro.product.brand"}, std::pair{"MODEL", "ro.product.model"},
        std::pair{"SECURITY_PATCH", "ro.build.version.security_patch"},
        std::pair{"DEVICE_INITIAL_SDK_INT", "ro.product.first_api_level"},
    }) {
        auto value = result.build.value(field, std::string{});
        if (!value.empty()) result.privateDefaults.emplace(property, std::move(value));
    }
}

Settings settings(nlohmann::json config, bool vending) {
    Settings result; result.vending=vending;
    auto take=[&](const char *key,int fallback) {
        auto value=config.value(key,std::string{});
        config.erase(key);
        if (value=="true") return 1;
        if (value=="false") return 0;
        return number(value,fallback);
    };
    result.fullInjection=take("fullInjection",1)!=0;
    const auto debug=take("DEBUG",0);
    result.verboseLogs=take("verboseLogs",debug);
    result.spoofBuild=take("spoofBuild",1); result.spoofProps=take("spoofProps",1);
    result.spoofProvider=take("spoofProvider",result.fullInjection ? 0 : 1); result.spoofSignature=take("spoofSignature",0);
    result.spoofVendingSdk=take("spoofVendingSdk",0);
    auto finger=config.value("spoofVendingFinger",std::string("0")); config.erase("spoofVendingFinger");
    const bool legacyFingerprint=!finger.empty() && finger!="0";
    if (finger.empty() || finger=="0" || finger=="1") {
        result.spoofVendingFinger=finger=="1";
        result.vendingFingerprint=config.value("FINGERPRINT",std::string());
        if(result.vendingFingerprint.empty()) result.spoofVendingFinger=0;
    } else { result.spoofVendingFinger=1; result.vendingFingerprint=finger; }
    const bool vendingBuild=take("spoofVendingBuild",!result.spoofVendingSdk && !legacyFingerprint)!=0;
    if (result.fullInjection) expandFingerprint(config);
    result.build=nlohmann::json::object();
    if(vending) {
        result.spoofBuild=result.spoofProps=result.spoofProvider=result.spoofSignature=0;
        result.nativeBuild=result.fullInjection && vendingBuild;
        if (!result.nativeBuild) return result;
    }
    result.spoofVendingFinger=result.spoofVendingSdk=0;
    for(const auto &entry:config.items()) {
        if(entry.key().find_first_of("*.")!=std::string::npos) {
            if(!vending && !entry.value().get<std::string>().empty()) result.properties.emplace(entry.key(),entry.value().get<std::string>());
        } else result.build[entry.key()]=entry.value();
    }
    if (!vending && result.fullInjection) {
        result.nativeBuild=result.spoofBuild && !result.spoofProvider && !result.spoofSignature;
        if (result.spoofProps) fillPrivateDefaults(result);
    }
    return result;
}
}
