#!/system/bin/sh
# SPDX-License-Identifier: GPL-3.0-only
# Derived from Play Integrity Fork boot scripts (23ec56c2440c4fd1a5e893a23ef7861d6366a828).
# Modified 2026-10-24: component-owned state and root-manager lifecycle integration.
set -eu
MODPATH="$PIF_ENGINE_ROOT"
export MODPATH PIF_ENGINE_DATA PIF_ENGINE_SCRIPTS PIF_ENGINE_MODE

# Installer shell functions do not cross the standalone process boundary.
ui_print() { printf '%s\n' "$*"; }

# Root managers provide a consistent BusyBox ash and resetprop implementation.
for bin in /data/adb/ksu/bin /data/adb/ap/bin /data/adb/magisk; do
  [ -x "$bin/resetprop" ] || continue
  PATH="$bin:$PATH"
  export PATH
  break
done

. "$PIF_ENGINE_SCRIPTS/path_rules.sh"
. "$PIF_ENGINE_SCRIPTS/power.sh"

cleanup_overlays() {
  manifest="$PIF_ENGINE_DATA/overlay-paths.txt"
  [ -f "$manifest" ] || return 0
  failed=false
  while IFS= read -r relative; do
    case "$relative" in system/*) ;; *) continue ;; esac
    device_path=/${relative#system/}
    case "$device_path" in /app/*|/priv-app/*|/overlay/*) device_path=/system$device_path ;; esac
    case "$device_path" in
      /*/overlay/config/config.xml) pif_valid_app_path "${device_path%/config/config.xml}/Placeholder.apk" || continue ;;
      *) pif_valid_app_path "$device_path" || continue ;;
    esac
    target="$PIF_ENGINE_ROOT/$relative"
    if [ -d "$target" ]; then
      rm -f "$target/.replace"
      if ! rmdir "$target" 2>/dev/null; then
        setfattr -x trusted.overlay.opaque "$target" 2>/dev/null || true
        printf 'Retained nonempty ROM overlay: %s\n' "$target" >&2
        failed=true
      fi
    else
      rm -f "$target"
    fi
  done < "$manifest"
  $failed && return 1
  rm -f "$manifest"
}

phase=$1
case "$phase" in
  setup|early|late) pif_running || phase=cleanup ;;
esac

case "$phase:$PIF_ENGINE_MODE" in
  setup:full|early:full)
    list="$PIF_ENGINE_SCRIPTS/app_replace_list.txt"
    [ ! -f "$PIF_ENGINE_DATA/custom.app_replace_list.txt" ] || list="$PIF_ENGINE_DATA/custom.app_replace_list.txt"
    pif_validate_list "$list" || exit 1
    ;;
esac

case "$phase" in
  cleanup)
    cleanup_overlays
    if [ -f "$PIF_ENGINE_DATA/uninstall.sh" ]; then sh -e "$PIF_ENGINE_DATA/uninstall.sh" && rm -f "$PIF_ENGINE_DATA/uninstall.sh"; fi
    ;;
  setup)
    # Preserve user state across upgrades. No implicit import of another module's config.
    mkdir -p "$PIF_ENGINE_DATA"
    chmod 0700 "$PIF_ENGINE_DATA"
    cleanup_overlays
    if [ "$PIF_ENGINE_MODE" != "scripts-only" ]; then
      set +eu
      . "$PIF_ENGINE_SCRIPTS/common_func.sh"
      . "$PIF_ENGINE_SCRIPTS/common_setup.sh"
    fi
    ;;
  early)
    cleanup_overlays
    # Upstream relies on non-fatal resetprop and optional Magisk commands.
    set +eu
    . "$PIF_ENGINE_SCRIPTS/post-fs-data.sh"
    ;;
  late)
    [ "$(getprop sys.boot_completed)" = "1" ] || exit 1
    [ ! -f /data/adb/disable_prop_handler ] || exit 0
    set +eu
    . "$PIF_ENGINE_SCRIPTS/service.sh"
    ;;
  uninstall)
    cleanup_overlays
    if [ -f "$PIF_ENGINE_DATA/uninstall.sh" ]; then
      sh -e "$PIF_ENGINE_DATA/uninstall.sh"
    fi
    rm -rf "$PIF_ENGINE_DATA"
    ;;
  *) exit 2 ;;
esac
