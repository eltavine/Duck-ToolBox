// SPDX-License-Identifier: GPL-3.0-only
#pragma once
#include "settings.hpp"

namespace pif {
class PrivateProperties {
public:
    bool prepare(const Settings &settings);
    size_t size() const { return expected.size(); }
    size_t matching() const;
private:
    PropertyValues expected;
};
}
