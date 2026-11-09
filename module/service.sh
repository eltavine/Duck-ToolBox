#!/system/bin/sh
# late_start service mode (non-blocking). Repairs the runtime directory so module updates
# never wipe saved data. Magisk has no boot-completed stage, so there this script waits for
# boot completion and runs boot-completed.sh itself.

umask 077
MODPATH=${0%/*}

# shellcheck source=util_functions.sh
. "$MODPATH/util_functions.sh" || exit 1

repair_runtime

if [ "$KSU" != "true" ] && [ "$APATCH" != "true" ]; then
  (
    until [ "$(getprop sys.boot_completed)" = "1" ]; do
      sleep 2
    done
    sh "$MODPATH/boot-completed.sh"
  ) &
fi
