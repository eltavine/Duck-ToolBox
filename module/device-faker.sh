# shellcheck shell=sh
setup_device_faker() {
  [ -f "$MODPATH/device-faker/device-faker-engine" ] || return 0
  chmod 0755 "$MODPATH/device-faker/device-faker-engine" || return 1
  "$MODPATH/device-faker/device-faker-engine" stage setup >/dev/null 2>&1 || return 1
}
