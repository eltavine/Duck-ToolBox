// SPDX-License-Identifier: GPL-3.0-only
#pragma once
#include "zygisk.hpp"
#include <array>

namespace duck {
using Table = zygisk::internal::api_table;
using Abi = zygisk::internal::module_abi;
using Entry = void (*)(Table *, JNIEnv *);

struct Engine {
    Table proxy{};
    Table *host = nullptr;
    Abi *abi = nullptr;
    unsigned char id = 0;
    bool unload = true;
    bool called = false;
    void initialize(Table *, JNIEnv *, Entry, unsigned char);
    void pre(zygisk::AppSpecializeArgs *);
    void post(const zygisk::AppSpecializeArgs *);
    static bool capture(Table *, Abi *);
    static int companion(void *);
    static void option(void *, zygisk::Option);
    static int directory(void *);
    static uint32_t flags(void *);
};

// IDs are a versioned wire contract, independent of engine implementation details.
constexpr std::array<unsigned char, 4> prefix{'D', 'T', 'B', 1};
bool channel(int fd, unsigned char &id, bool writing);
}
