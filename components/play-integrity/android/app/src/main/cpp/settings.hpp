/* Derived from Play Integrity Fork, osm0sis, chiteroman and contributors.
 * Source: https://github.com/osm0sis/PlayIntegrityFork/tree/23ec56c2440c4fd1a5e893a23ef7861d6366a828
 * SPDX-License-Identifier: GPL-3.0-only
 * Modified 2026-10-23: independent engine packaging, bounded configuration and runtime operations.
 */
#pragma once
#include "transport.hpp"
#include <map>
namespace pif {
using PropertyValues = std::map<std::string,std::string,std::less<>>;
const std::string *propertyOverride(const PropertyValues &rules, std::string_view name);

struct Settings {
    bool vending=false;
    bool fullInjection=true, nativeBuild=false;
    int verboseLogs=0, spoofBuild=1, spoofProps=1, spoofProvider=1, spoofSignature=0;
    int spoofVendingFinger=0, spoofVendingSdk=0;
    std::string vendingFingerprint;
    nlohmann::json build;
    PropertyValues properties;
    PropertyValues privateDefaults;
    bool injectDex() const {
        return (!nativeBuild && spoofBuild) || spoofProvider || spoofSignature || spoofVendingFinger || spoofVendingSdk;
    }
    bool injectJava() const { return nativeBuild || injectDex(); }
};
Settings settings(nlohmann::json config, bool vending);
}
