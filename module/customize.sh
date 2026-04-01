#!/system/bin/sh

(
  umask 077

  MODULE_ID="duck-toolbox"
  DATA_ROOT="${DUCK_TOOLBOX_DATA_ROOT:-/data/adb/$MODULE_ID}"
  VAR_DIR="$DATA_ROOT/var"

  . "$MODPATH/util_functions.sh"

  repair_runtime
) || abort "! Duck ToolBox install customization failed."
