# shellcheck shell=sh
# Apply saved boot profiles after Android has initialized swap.
( "$DUCKCTL" zram boot-apply >/dev/null 2>&1 || true ) &
( "$DUCKCTL" linux-env boot-apply >/dev/null 2>&1 || true ) &
