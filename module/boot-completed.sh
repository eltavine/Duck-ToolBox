#!/system/bin/sh
# Runs once boot has completed and `pm` works (KernelSU and APatch call it directly; on
# Magisk service.sh does). Nothing here may block or fail the boot.

MODPATH=${0%/*}
DUCKCTL="$MODPATH/bin/duckctl.sh"
TSPA_DIR="/data/adb/modules/tsupport-advance"
TSPA_FLAG="/storage/emulated/0/stop-tspa-auto-target"

# Late pass for props the system sets after post-fs-data.
sh "$MODPATH/prop.sh"
if [ -x "$MODPATH/play-integrity/pif-engine" ]; then
  PIF_ENGINE_MODULE="$MODPATH/play-integrity" \
    "$MODPATH/play-integrity/pif-engine" stage late >/dev/null 2>&1 \
    || sh "$MODPATH/prop.sh" --fallback
fi

"$DUCKCTL" tricky-store entry apply >/dev/null 2>&1 || true
"$DUCKCTL" tricky-store auto-apply >/dev/null 2>&1 || true
"$DUCKCTL" describe >/dev/null 2>&1 || true

# Apply only explicitly saved ZRAM boot profiles after Android has initialized swap.
( "$DUCKCTL" zram boot-apply >/dev/null 2>&1 || true ) &
( "$DUCKCTL" linux-env boot-apply >/dev/null 2>&1 || true ) &

# TSupport-Advance rewrites the target list on its own unless this flag exists (as Tricky
# Addon does). Shared storage appears only after the user unlocks the device.
tries=0
until [ -d /storage/emulated/0/Android ] || [ "$tries" -ge 90 ]; do
  sleep 2
  tries=$((tries + 1))
done
if [ -d "$TSPA_DIR" ]; then
  touch "$TSPA_FLAG" 2>/dev/null || true
else
  rm -f "$TSPA_FLAG"
fi
