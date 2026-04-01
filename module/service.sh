#!/system/bin/sh

umask 077

[ -z "$MODPATH" ] && MODPATH=${0%/*}

if [ -z "${DUCK_TOOLBOX_BUSYBOX_REEXEC:-}" ] && [ -x /data/adb/ksu/bin/busybox ]; then
  export DUCK_TOOLBOX_BUSYBOX_REEXEC=1
  export ASH_STANDALONE=1
  exec /data/adb/ksu/bin/busybox sh "$0" "$@"
fi

MODULE_ID="duck-toolbox"
DATA_ROOT="${DUCK_TOOLBOX_DATA_ROOT:-/data/adb/$MODULE_ID}"
VAR_DIR="$DATA_ROOT/var"

. "$MODPATH/util_functions.sh"

repair_runtime
