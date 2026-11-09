# shellcheck shell=sh
# Sourced by customize.sh and service.sh after they set MODPATH.

# shellcheck source=identity.sh
. "$MODPATH/identity.sh" || return 1
MODULE_ID=$(duck_module_id "$MODPATH") || return 1
DATA_ROOT="${DUCK_TOOLBOX_DATA_ROOT:-/data/adb/$MODULE_ID}"
VAR_DIR="$DATA_ROOT/var"

dir_has_entries() {
  dir="$1"
  [ -d "$dir" ] || return 1
  find "$dir" -mindepth 1 -print -quit 2>/dev/null | grep -q .
}

runtime_data_has_content() {
  base_dir="$1"
  [ -f "$base_dir/profile.toml" ] && return 0
  [ -f "$base_dir/profile.secrets.toml" ] && return 0
  dir_has_entries "$base_dir/outputs" && return 0
  dir_has_entries "$base_dir/logs" && return 0
  return 1
}

copy_tree_contents() {
  source_dir="$1"
  target_dir="$2"
  [ -d "$source_dir" ] || return 0
  mkdir -p "$target_dir"
  cp -a "$source_dir/." "$target_dir/" 2>/dev/null \
    || cp -R "$source_dir/." "$target_dir/" 2>/dev/null \
    || true
}

copy_file_if_missing() {
  source_file="$1"
  target_file="$2"
  [ -f "$source_file" ] || return 0
  [ -e "$target_file" ] && return 0
  mkdir -p "$(dirname -- "$target_file")"
  cat "$source_file" > "$target_file"
}

prepare_data_root() {
  mkdir -p "$DATA_ROOT" "$VAR_DIR" "$VAR_DIR/outputs" "$VAR_DIR/tmp" "$VAR_DIR/logs"
}

maybe_migrate_legacy_var() {
  for source_var in \
    "$MODPATH/var" \
    "/data/adb/modules/$MODULE_ID/var" \
    "/data/adb/modules_update/$MODULE_ID/var"
  do
    [ -d "$source_var" ] || continue
    [ "$source_var" = "$VAR_DIR" ] && continue

    if runtime_data_has_content "$source_var" && ! runtime_data_has_content "$VAR_DIR"; then
      copy_tree_contents "$source_var" "$VAR_DIR"
    fi

    copy_file_if_missing "$source_var/profile.toml" "$VAR_DIR/profile.toml"
    copy_file_if_missing "$source_var/profile.secrets.toml" "$VAR_DIR/profile.secrets.toml"
    copy_file_if_missing "$source_var/logs/duckd.log" "$VAR_DIR/logs/duckd.log"

    if dir_has_entries "$source_var/outputs" && ! dir_has_entries "$VAR_DIR/outputs"; then
      copy_tree_contents "$source_var/outputs" "$VAR_DIR/outputs"
    fi
  done
}

# Runs from the installer and from service.sh on every boot, so it cannot rely on the
# installer-only set_perm helpers.
repair_runtime() {
  prepare_data_root
  maybe_migrate_legacy_var

  for binary in "$MODPATH/bin/duckctl.sh" "$MODPATH/bin/duckd"; do
    [ -f "$binary" ] && chmod 0755 "$binary"
  done
  chown -R 0:0 "$DATA_ROOT" 2>/dev/null
  chmod 0700 "$DATA_ROOT"
  find "$VAR_DIR" -type d -exec chmod 0700 {} \; 2>/dev/null
  find "$VAR_DIR" -type f -exec chmod 0600 {} \; 2>/dev/null
}

# Tricky Addon's boot hash format: one lowercase hex string; an empty file is removed.
normalize_boot_hash() {
  file="/data/adb/boot_hash"
  [ -f "$file" ] || return 0
  hash_value="$(grep -v '^[[:space:]]*#' "$file" | tr -d '[:space:]' | tr '[:upper:]' '[:lower:]')"
  if [ -n "$hash_value" ]; then
    printf '%s\n' "$hash_value" >"$file"
  else
    rm -f "$file"
  fi
}
