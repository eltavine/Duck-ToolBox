// SPDX-License-Identifier: GPL-3.0-only
#pragma once
#include <jni.h>
#include <string>
#include <string_view>
#include <vector>

namespace duck::language {
struct Target {
    unsigned uid = 0;
    std::string package;
};
bool target(unsigned uid, std::string_view process, std::string_view directory, Target &result);
std::string select(std::string_view json, const Target &target);
bool active(std::string_view);
bool exchange(int fd, const Target &, std::string &, std::vector<char> &);
void companion(int fd);
// Returns true once ART has resident callbacks, even if an optional hook fails.
bool install(JNIEnv *, const std::string &, std::vector<char> &);
}
