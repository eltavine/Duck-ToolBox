// SPDX-License-Identifier: GPL-3.0-only
#pragma once

namespace duck {
// Switch to normal pthread destructors before starting process-lifetime work.
void keep_pthread_keys();
bool pthread_keys_allow_unload();
}
