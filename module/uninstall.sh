#!/system/bin/sh
# Runs when the root manager removes the module (not on updates). Removes everything Duck
# ToolBox created, as Tricky Addon's uninstall does, including saved RKP key material.

MODDIR=${0%/*}
MODULE_ID="duck-toolbox"
DATA_ROOT="/data/adb/$MODULE_ID"

# Keep environment data if its processes or import worker cannot be stopped safely.
if [ -d "$DATA_ROOT/var/linux-env" ]; then
  if ! "$MODDIR/bin/duckctl.sh" linux-env stop-all >/dev/null 2>&1; then
    echo "Linux environments could not be stopped; preserving $DATA_ROOT" >&2
    exit 1
  fi
fi

if [ -x "$MODDIR/play-integrity/pif-engine" ]; then
  "$MODDIR/play-integrity/pif-engine" stage uninstall >/dev/null 2>&1 || true
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

rm -f /storage/emulated/0/stop-tspa-auto-target
rm -f /data/adb/disable_prop_handler /data/adb/boot_hash
rm -rf "$DATA_ROOT"
