/* Derived from Play Integrity Fork, osm0sis, chiteroman and contributors.
 * Source: https://github.com/osm0sis/PlayIntegrityFork/tree/23ec56c2440c4fd1a5e893a23ef7861d6366a828
 * SPDX-License-Identifier: GPL-3.0-only
 * Modified 2026-10-23: independent engine packaging, bounded configuration and runtime operations.
 */
#pragma once
#include "settings.hpp"
#include <sys/system_properties.h>
namespace pif {
class PropertyHook {
public:
    bool install(const Settings &settings);
private:
    using Callback=void (*)(void *,const char *,const char *,uint32_t);
    using Read=void (*)(const prop_info *,Callback,void *);
    struct Context { const PropertyHook *owner; Callback callback; void *cookie; };
    static void read(const prop_info *,Callback,void *);
    static void modify(void *,const char *,const char *,uint32_t);
    PropertyValues properties;
    Read original=nullptr;
    int logLevel=0;
};
}
