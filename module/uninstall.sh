#!/system/bin/sh
# Runs when the root manager removes the module (not on updates). Removes everything Duck
# ToolBox created, as Tricky Addon's uninstall does, including saved RKP key material.

MODDIR=${0%/*}
# shellcheck source=identity.sh
. "$MODDIR/identity.sh" || exit 1
MODULE_ID=$(duck_module_id "$MODDIR") || exit 1
DATA_ROOT="/data/adb/$MODULE_ID"

# Keep environment data if its processes or import worker cannot be stopped safely.
if [ "$MODULE_ID" = "duck-toolbox" ] && [ -d "$DATA_ROOT/var/linux-env" ]; then
  if ! "$MODDIR/bin/duckctl.sh" linux-env stop-all >/dev/null 2>&1; then
    echo "Linux environments could not be stopped; preserving $DATA_ROOT" >&2
    exit 1
  fi
fi

if [ -x "$MODDIR/play-integrity/pif-engine" ]; then
  "$MODDIR/play-integrity/pif-engine" stage uninstall >/dev/null 2>&1 || true
fi

if [ -x "$MODDIR/device-faker/device-faker-engine" ]; then
  if "$MODDIR/device-faker/device-faker-engine" stage uninstall >/dev/null 2>&1; then
    rm -rf /data/adb/duck-device-faker
  else
    echo "Device Faker density recovery is incomplete; preserving /data/adb/duck-device-faker" >&2
  fi
fi

# Entry links in keystore modules; the shell fallback covers a missing or broken binary.
"$MODDIR/bin/duckctl.sh" tricky-store entry remove >/dev/null 2>&1
for module in tricky_store oh_my_keymint teesim; do
  for name in webroot action.sh; do
    link="/data/adb/modules/$module/$name"
    [ -L "$link" ] || continue
    case "$(readlink "$link")" in
      "$MODDIR"/* | "/data/adb/modules/$MODULE_ID"/*) rm -f "$link" ;;
    esac
  done
done

if ! conflicting_installation "$MODULE_ID" /data/adb; then
  rm -f /storage/emulated/0/stop-tspa-auto-target
  rm -f /data/adb/disable_prop_handler /data/adb/boot_hash
fi
rm -rf "$DATA_ROOT"
