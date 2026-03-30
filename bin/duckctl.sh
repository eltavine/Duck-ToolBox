#!/system/bin/sh
set -eu
umask 077

if [ -z "${DUCK_TOOLBOX_BUSYBOX_REEXEC:-}" ] && [ -x /data/adb/ksu/bin/busybox ]; then
  export DUCK_TOOLBOX_BUSYBOX_REEXEC=1
  export ASH_STANDALONE=1
  exec /data/adb/ksu/bin/busybox sh "$0" "$@"
fi

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
MODULE_ROOT="${DUCK_TOOLBOX_ROOT:-$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)}"
case "$MODULE_ROOT" in
  /data/adb/modules/*|/data/adb/modules_update/*)
    DEFAULT_DATA_ROOT="/data/adb/duck-toolbox"
    ;;
  *)
    DEFAULT_DATA_ROOT="$MODULE_ROOT"
    ;;
esac
DATA_ROOT="${DUCK_TOOLBOX_DATA_ROOT:-$DEFAULT_DATA_ROOT}"
WANTS_JSON=0
SEARCHED_CANDIDATES=""

for arg in "$@"
do
  if [ "$arg" = "--json" ]; then
    WANTS_JSON=1
    break
  fi
done

for candidate in \
  "$MODULE_ROOT/bin/duckd" \
  "$MODULE_ROOT/duckd/target/aarch64-linux-android/release/duckd" \
  "$MODULE_ROOT/duckd/target/aarch64-linux-android/debug/duckd" \
  "$MODULE_ROOT/duckd/target/release/duckd" \
  "$MODULE_ROOT/duckd/target/debug/duckd" \
  "$MODULE_ROOT"/duckd/target/*-linux-android/release/duckd \
  "$MODULE_ROOT"/duckd/target/*-linux-android/debug/duckd
do
  if [ -n "$SEARCHED_CANDIDATES" ]; then
    SEARCHED_CANDIDATES="$SEARCHED_CANDIDATES, "
  fi
  SEARCHED_CANDIDATES="${SEARCHED_CANDIDATES}${candidate}"

  if [ -f "$candidate" ] && [ ! -x "$candidate" ]; then
    chmod 700 "$candidate" 2>/dev/null || true
  fi

  if [ -x "$candidate" ]; then
    export DUCK_TOOLBOX_ROOT="$MODULE_ROOT"
    export DUCK_TOOLBOX_DATA_ROOT="$DATA_ROOT"
    if "$candidate" "$@"; then
      exit 0
    fi

    status=$?
    if [ "$status" -eq 126 ] || [ "$status" -eq 127 ]; then
      continue
    fi

    exit "$status"
  fi
done

if [ "$WANTS_JSON" -eq 1 ]; then
  printf '%s\n' "{\"ok\":false,\"command\":\"bootstrap.wrapper\",\"data\":null,\"error\":{\"code\":\"binary_not_found\",\"message\":\"Duck ToolBox backend binary is missing or not executable.\",\"details\":{\"module_root\":\"$MODULE_ROOT\",\"searched\":\"$SEARCHED_CANDIDATES\"}}}"
else
  echo "Duck ToolBox backend binary is missing or not executable under $MODULE_ROOT" >&2
  echo "searched: $SEARCHED_CANDIDATES" >&2
fi

exit 127
