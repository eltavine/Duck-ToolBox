// SPDX-License-Identifier: GPL-3.0-only
#pragma once
#include <jni.h>
#include <string>
namespace pif {
void observe(JNIEnv *env, const std::string &raw, const std::string &process,
             const std::string &boot, const std::string &start, bool propertyHook,
             size_t privateRequested = 0, size_t privateMatching = 0);
std::string processStart();
std::string bootIdentity();
}
