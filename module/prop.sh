#!/system/bin/sh
# Sensitive-prop handler (ported from Tricky Addon). Early passes only change properties
# that PIF also handles before boot; OEM lock state and AVB metadata wait until boot
# completes. Only `resetprop -n` is used to avoid init property-service side effects.

EARLY=false
FALLBACK=false
PROP_STATUS=0
for option in "$@"; do
  case "$option" in
    --early) EARLY=true ;;
    --fallback) FALLBACK=true ;;
  esac
done

DISABLE_FLAG="/data/adb/disable_prop_handler"
BOOT_HASH_FILE="/data/adb/boot_hash"

resetprop_bin=""
for candidate in /data/adb/ksu/bin/resetprop /data/adb/ap/bin/resetprop /data/adb/magisk/resetprop; do
  if [ -x "$candidate" ]; then
    resetprop_bin="$candidate"
    break
  fi
done
if [ -z "$resetprop_bin" ]; then
  printf '%s\n' 'Prop handler: resetprop is unavailable' >&2
  exit 1
fi

# The Verified Boot hash from Key Attestation applies even with the handler disabled.
if ! $EARLY && [ -f "$BOOT_HASH_FILE" ]; then
  hash_value="$(grep -v '^[[:space:]]*#' "$BOOT_HASH_FILE" | tr -d '[:space:]' | tr '[:upper:]' '[:lower:]')"
  case "$hash_value" in
    '') rm -f "$BOOT_HASH_FILE" ;;
    *[!0-9a-f]*) printf '%s\n' 'Prop handler: ignoring invalid boot hash' >&2 ;;
    *)
      case "${#hash_value}" in
        64|128) "$resetprop_bin" -n ro.boot.vbmeta.digest "$hash_value" || PROP_STATUS=1 ;;
        *) printf '%s\n' 'Prop handler: ignoring invalid boot hash length' >&2 ;;
      esac
      ;;
  esac
fi

[ -f "$DISABLE_FLAG" ] && exit "$PROP_STATUS"

# A failed engine stage requests the original handler while still honoring its switch.
PIF_ENGINE="${0%/*}/play-integrity/pif-engine"
if ! $FALLBACK && [ -x "$PIF_ENGINE" ] \
  && PIF_ENGINE_MODULE="${PIF_ENGINE%/*}" "$PIF_ENGINE" active; then
  exit "$PROP_STATUS"
fi

# Replace the value when the prop exists and differs from the expected one.
reset_expected() {
  current="$("$resetprop_bin" "$1")" || { PROP_STATUS=1; return; }
  if [ -n "$current" ] && [ "$current" != "$2" ]; then
    "$resetprop_bin" -n "$1" "$2" || PROP_STATUS=1
  fi
}

# Replace the value when it contains a marker (e.g. a recovery boot mode).
reset_if_contains() {
  current="$("$resetprop_bin" "$1")" || { PROP_STATUS=1; return; }
  case "$current" in
    *"$2"*) "$resetprop_bin" -n "$1" "$3" || PROP_STATUS=1 ;;
  esac
}

# Fill in the value when the prop is empty.
reset_if_empty() {
  current="$("$resetprop_bin" "$1")" || { PROP_STATUS=1; return; }
  [ -n "$current" ] || "$resetprop_bin" -n "$1" "$2" || PROP_STATUS=1
}

reset_expected ro.boot.warranty_bit 0
reset_expected ro.warranty_bit 0
reset_expected ro.debuggable 0
reset_expected ro.force.debuggable 0
reset_expected ro.secure 1
reset_expected ro.adb.secure 1
reset_expected ro.build.type user
reset_expected ro.build.tags release-keys
reset_expected ro.vendor.boot.warranty_bit 0
reset_expected ro.vendor.warranty_bit 0
reset_expected ro.boot.realmebootstate green

$EARLY && exit "$PROP_STATUS"

reset_expected ro.boot.vbmeta.device_state locked
reset_expected ro.boot.verifiedbootstate green
reset_expected ro.boot.flash.locked 1
reset_expected ro.boot.veritymode enforcing
reset_expected vendor.boot.vbmeta.device_state locked
reset_expected vendor.boot.verifiedbootstate green
reset_expected sys.oem_unlock_allowed 0

# MIUI
reset_expected ro.secureboot.lockstate locked

# Realme
reset_expected ro.boot.realme.lockstate 1

# Hide a boot from recovery (Magisk in recovery mode).
reset_if_contains ro.bootmode recovery unknown
reset_if_contains ro.boot.bootmode recovery unknown
reset_if_contains vendor.boot.bootmode recovery unknown

# vbmeta props an AVB-verified boot always has.
reset_if_empty ro.boot.vbmeta.device_state locked
reset_if_empty ro.boot.vbmeta.invalidate_on_error yes
reset_if_empty ro.boot.vbmeta.avb_version 1.0
digest="$("$resetprop_bin" ro.boot.vbmeta.digest)" || PROP_STATUS=1
hash_alg=sha256
case "$digest" in
  *[!0-9a-fA-F]*) ;;
  *) [ "${#digest}" -ne 128 ] || hash_alg=sha512 ;;
esac
reset_if_empty ro.boot.vbmeta.hash_alg "$hash_alg"
reset_if_empty ro.boot.vbmeta.size 4096
exit "$PROP_STATUS"
