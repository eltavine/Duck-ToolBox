#!/system/bin/sh

(
  set -eu
  umask 077

  MODULE_ID="duck-toolbox"
  DATA_ROOT="${DUCK_TOOLBOX_DATA_ROOT:-/data/adb/$MODULE_ID}"
  VAR_DIR="$DATA_ROOT/var"

  normalize_dir() {
    candidate="$1"
    [ -n "$candidate" ] || return 1
    [ -d "$candidate" ] || return 1
    CDPATH= cd -- "$candidate" 2>/dev/null && pwd -P
  }

  resolve_moddir() {
    for candidate in \
      "${MODPATH:-}" \
      "${DUCK_TOOLBOX_ROOT:-}" \
      "/data/adb/modules_update/$MODULE_ID" \
      "/data/adb/modules/$MODULE_ID"
    do
      [ -n "$candidate" ] || continue
      if [ -f "$candidate/module.prop" ]; then
        normalize_dir "$candidate"
        return 0
      fi
    done

    if [ -n "${0:-}" ] && [ "${0#*/}" != "$0" ]; then
      search_dir="$(normalize_dir "$(dirname -- "$0")" 2>/dev/null || true)"
      while [ -n "$search_dir" ]; do
        if [ -f "$search_dir/module.prop" ]; then
          printf '%s\n' "$search_dir"
          return 0
        fi
        parent="$(dirname -- "$search_dir")"
        [ "$parent" = "$search_dir" ] && break
        search_dir="$parent"
      done
    fi

    return 1
  }

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
    cp -p "$source_file" "$target_file" 2>/dev/null \
      || cp "$source_file" "$target_file" 2>/dev/null \
      || true
  }

  prepare_data_root() {
    mkdir -p "$DATA_ROOT" "$VAR_DIR" "$VAR_DIR/outputs" "$VAR_DIR/tmp" "$VAR_DIR/logs"
  }

  maybe_migrate_legacy_var() {
    for source_var in \
      "$MODDIR/var" \
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

  MODDIR="$(resolve_moddir || true)"
  if [ -z "$MODDIR" ]; then
    MODDIR="${MODPATH:-${DUCK_TOOLBOX_ROOT:-/data/adb/modules_update/$MODULE_ID}}"
  fi

  repair_runtime() {
    prepare_data_root
    maybe_migrate_legacy_var

    if command -v chown >/dev/null 2>&1; then
      chown 0:0 "$MODDIR/bin/duckctl.sh" "$MODDIR/bin/duckd" 2>/dev/null \
        || chown 0.0 "$MODDIR/bin/duckctl.sh" "$MODDIR/bin/duckd" 2>/dev/null \
        || true
    fi

    chmod 0755 "$MODDIR/bin/duckctl.sh" 2>/dev/null || true
    chmod 0755 "$MODDIR/bin/duckd" 2>/dev/null || true

    if command -v find >/dev/null 2>&1 && [ -d "$VAR_DIR" ]; then
      if command -v chown >/dev/null 2>&1; then
        chown 0:0 "$DATA_ROOT" "$VAR_DIR" "$VAR_DIR/outputs" "$VAR_DIR/tmp" "$VAR_DIR/logs" 2>/dev/null \
          || chown 0.0 "$DATA_ROOT" "$VAR_DIR" "$VAR_DIR/outputs" "$VAR_DIR/tmp" "$VAR_DIR/logs" 2>/dev/null \
          || true
      fi
      find "$VAR_DIR" -type d -exec chmod 0700 {} \; 2>/dev/null || true
      find "$VAR_DIR" -type f -exec chmod 0600 {} \; 2>/dev/null || true
    else
      if command -v chown >/dev/null 2>&1; then
        chown 0:0 \
          "$DATA_ROOT" \
          "$VAR_DIR" \
          "$VAR_DIR/outputs" \
          "$VAR_DIR/tmp" \
          "$VAR_DIR/logs" 2>/dev/null \
          || chown 0.0 \
            "$DATA_ROOT" \
            "$VAR_DIR" \
            "$VAR_DIR/outputs" \
            "$VAR_DIR/tmp" \
            "$VAR_DIR/logs" 2>/dev/null \
          || true
      fi
      chmod 0700 \
        "$DATA_ROOT" \
        "$VAR_DIR" \
        "$VAR_DIR/outputs" \
        "$VAR_DIR/tmp" \
        "$VAR_DIR/logs" 2>/dev/null || true
      chmod 0600 \
        "$VAR_DIR/profile.toml" \
        "$VAR_DIR/profile.secrets.toml" \
        "$VAR_DIR/logs/duckd.log" 2>/dev/null || true
    fi
  }

  repair_runtime
) || {
  status=$?
  if command -v abort >/dev/null 2>&1; then
    abort "! Duck ToolBox install customization failed."
  fi
  return "$status" 2>/dev/null || exit "$status"
}
