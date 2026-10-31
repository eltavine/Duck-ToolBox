// SPDX-License-Identifier: GPL-3.0-only
#include "language.hpp"
#include "json/single_include/nlohmann/json.hpp"
#include <algorithm>
#include <charconv>
#include <cctype>
#include <set>

namespace duck::language {
static bool package_name(std::string_view name) {
    if (name.empty() || name.size() > 255 || name.find('.') == name.npos) return false;
    bool first = true;
    for (unsigned char c : name) {
        if (c == '.') { if (first) return false; first = true; continue; }
        if ((first && !std::isalpha(c)) || (!std::isalnum(c) && c != '_') || c > 127) return false;
        first = false;
    }
    return !first;
}
bool target(unsigned uid, std::string_view process, std::string_view directory, Target &result) {
    // Reject system, SDK sandbox and isolated UIDs; data-directory identity prevents
    // process aliases or shared UIDs from selecting another package's rule.
    if (uid % 100000 < 10000 || uid % 100000 >= 20000) return false;
    const auto split = directory.rfind('/');
    if (split == directory.npos) return false;
    auto package = directory.substr(split + 1);
    if (!package_name(package) || !(process == package || (process.size() > package.size() + 1 && process.starts_with(std::string(package) + ":")))) return false;
    auto parent = directory.substr(0, split);
    unsigned user = 0;
    if (parent != "/data/data") {
        const auto slash = parent.rfind('/');
        if (slash == parent.npos) return false;
        auto root = parent.substr(0, slash);
        if (root != "/data/user" && root != "/data/user_de") {
            // Adoptable storage: /mnt/expand/<volume>/user[_de]/<user>/<package>.
            if (!root.starts_with("/mnt/expand/")) return false;
            auto mounted = root.substr(12);
            const auto separator = mounted.find('/');
            if (separator == mounted.npos || separator == 0) return false;
            auto volume = mounted.substr(0, separator);
            auto kind = mounted.substr(separator + 1);
            if (volume == "." || volume == ".." || (kind != "user" && kind != "user_de")) return false;
        }
        auto number = parent.substr(slash + 1);
        auto parsed = std::from_chars(number.data(), number.data() + number.size(), user);
        if (parsed.ec != std::errc{} || parsed.ptr != number.data() + number.size()) return false;
    }
    if (user != uid / 100000) return false;
    result = {uid, std::string(package)};
    return true;
}
static std::string locale(std::string_view tag) {
    // Backend emits canonical language[-Script][-REGION]. Reject malformed or
    // hand-edited data before handing it to Locale.forLanguageTag.
    if (tag.size() < 2 || tag.size() > 12) return {};
    std::vector<std::string_view> parts;
    for (size_t start = 0; start <= tag.size();) {
        auto end = tag.find('-', start);
        parts.push_back(tag.substr(start, end == tag.npos ? end : end - start));
        if (end == tag.npos) break;
        start = end + 1;
    }
    auto alpha = [](std::string_view p) { return std::all_of(p.begin(), p.end(), [](unsigned char c) { return c < 128 && std::isalpha(c); }); };

    if ((parts[0].size() != 2 && parts[0].size() != 3) || !alpha(parts[0])) return {};
    size_t i = 1;
    if (i < parts.size() && parts[i].size() == 4 && alpha(parts[i])) ++i;
    if (i < parts.size()) {
        auto p = parts[i++];
        if (!((p.size() == 2 && alpha(p)) || (p.size() == 3 && std::all_of(p.begin(), p.end(), [](unsigned char c) { return std::isdigit(c); })))) return {};
    }
    if (i != parts.size()) return {};
    std::string result;
    for (size_t index = 0; index < parts.size(); ++index) {
        std::string part(parts[index]);
        for (auto &c : part) c = static_cast<char>(index == 0 ? std::tolower(static_cast<unsigned char>(c))
            : std::toupper(static_cast<unsigned char>(c)));
        if (index > 0 && part.size() == 4)
            for (size_t j = 1; j < part.size(); ++j) part[j] = static_cast<char>(std::tolower(static_cast<unsigned char>(part[j])));
        if (index == 0 && part == "und") return {};
        if (!result.empty()) result += '-';
        result += part;
    }
    return result;
}
bool active(std::string_view value) {
    return value.size() > 2 && value.size() <= 64 && value.starts_with("1:")
        && std::all_of(value.begin() + 2, value.end(), [](unsigned char c) {
            return c < 128 && (std::isalnum(c) || c == '.' || c == '_' || c == '-');
        });
}
std::string select(std::string_view text, const Target &target) {
    bool valid = true;
    std::vector<std::set<std::string>> keys;
    auto callback = [&](int depth, nlohmann::json::parse_event_t event, nlohmann::json &value) {
        if (!valid || depth > 8) { valid = false; return false; }
        if (event == nlohmann::json::parse_event_t::object_start) keys.emplace_back();
        else if (event == nlohmann::json::parse_event_t::object_end) { if (!keys.empty()) keys.pop_back(); }
        else if (event == nlohmann::json::parse_event_t::key) {
            if (keys.empty() || !keys.back().insert(value.get<std::string>()).second) valid = false;
        }
        return valid;
    };
    auto config = nlohmann::json::parse(text, callback, false);
    if (!valid) return {};
    if (!config.is_object() || config.size() != 1 || !config.contains("rules") || !config["rules"].is_array() || config["rules"].size() > 256) return {};
    std::string selected;
    std::set<std::pair<unsigned, std::string>> targets;
    for (const auto &rule : config["rules"]) {
        if (!rule.is_object() || rule.size() != 3 || !rule.contains("package_name") || !rule["package_name"].is_string()
            || !rule.contains("user_id") || !rule["user_id"].is_number_unsigned() || !rule.contains("locale") || !rule["locale"].is_string()) return {};
        const auto &name = rule["package_name"].get_ref<const std::string &>();
        auto tag = locale(rule["locale"].get_ref<const std::string &>());
        if (!package_name(name) || tag.empty() || rule["user_id"].get<uint64_t>() > 21473) return {};
        if (!targets.emplace(rule["user_id"].get<unsigned>(), name).second) return {};
        if (name == target.package && rule["user_id"].get<unsigned>() == target.uid / 100000) {
            if (!selected.empty()) return {};
            selected = tag;
        }
    }
    return selected;
}
}
