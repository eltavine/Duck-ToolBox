# ZRAM and swap

The `zram` feature manages existing ZRAM block devices used as swap. Other swap
partitions and files are listed read-only. It neither creates swap files nor edits
fstab, init scripts, Android properties, SELinux policy, or memory-manager settings.

## Source baseline

Implementation references:

- [ACK android15-6.6 ZRAM driver](https://android.googlesource.com/kernel/common/+/aa8f225d2792f5fd45e3a853fc2fd5d2e7baf8af/drivers/block/zram/zram_drv.c):
  `comp_algorithm`, initialization, memory counters, secondary compressors, writeback,
  and `reset_store()` refusing open or claimed devices.
- [ACK ZRAM sysfs ABI](https://android.googlesource.com/kernel/common/+/aa8f225d2792f5fd45e3a853fc2fd5d2e7baf8af/Documentation/ABI/testing/sysfs-block-zram)
  and [upstream ZRAM documentation](https://docs.kernel.org/admin-guide/blockdev/zram.html):
  counter order and units, selected algorithm syntax, reset semantics and swap setup.
- [ACK swap implementation](https://android.googlesource.com/kernel/common/+/aa8f225d2792f5fd45e3a853fc2fd5d2e7baf8af/mm/swapfile.c):
  permission checks, swapoff memory/error handling, automatic and explicit priorities.
- [AArch64 generic syscall numbers](https://android.googlesource.com/kernel/common/+/aa8f225d2792f5fd45e3a853fc2fd5d2e7baf8af/include/uapi/asm-generic/unistd.h):
  `swapon=224`, `swapoff=225`.
  [ARM EABI](https://android.googlesource.com/kernel/common/+/aa8f225d2792f5fd45e3a853fc2fd5d2e7baf8af/arch/arm/tools/syscall.tbl)
  uses different numbers (`87`, `115`). Mutations are enabled only on Android ARM64,
  use target libc wrappers and assert the ARM64 constants at compilation. Host and
  ARM32 builds can browse but cannot enter the mutation path.
- [AOSP fs_mgr](https://android.googlesource.com/platform/system/core/+/a3b721a32242006b59cb12bd62c9133632af3a2d/fs_mgr/fs_mgr.cpp):
  ZRAM setup, `/system/bin/mkswap`, priority flags and the transition to `mmd`.
- [AOSP Toybox mkswap](https://android.googlesource.com/platform/external/toybox/+/59a869a838cfadbf6bc41cb400471ca9d2cae9b5/toys/other/mkswap.c):
  swap header formatting with the runtime page size. The platform formatter is used
  directly without a shell. This feature does not implement its own swap format.
- [Material 3 dialogs](https://m3.material.io/components/dialogs/guidelines),
  [Android dialog guidance](https://developer.android.com/develop/ui/compose/components/dialog),
  [Apple alerts](https://developer.apple.com/design/human-interface-guidelines/alerts),
  [Apple accessibility](https://developer.apple.com/design/human-interface-guidelines/accessibility):
  explicit action labels, confirmation for resetting swap, persistent in-context
  feedback, readable layouts and keyboard focus restoration.

## Operation contract

`duckd zram status` returns physical/available memory, global `pswpin`/`pswpout`
page counts, `/proc/swaps`, ZRAM device identity, algorithm capabilities and memory
statistics. Compressed data size and allocated physical memory are separate values.
The compressed/original ratio is absent for an empty device. Page counts use the
running kernel's page size, not an assumed 4096 bytes.

An active swap device is associated with ZRAM using block-device major/minor identity,
including aliased paths. An unresolved active partition blocks resets. Missing or
malformed required state fails closed. Missing optional statistics remain unknown.

Runtime operations require Android ARM64, effective UID 0, the real `/` sysroot,
explicit confirmation and a configuration revision from the current boot. Input
rejects unknown fields. The UI and backend both validate size and priority; the
backend additionally validates the running page alignment and device identity.
The editor accepts whole MiB from 16 MiB to twice physical memory, or zero to disable.
This upper limit is application policy, not a kernel ABI limit. Restoration retains
the original byte size even when it is outside this editing policy.

Before deactivation, available memory must exceed used swap bytes plus the larger
of 256 MiB or 10% of physical memory. This conservative check cannot guarantee
successful swapoff or prevent concurrent memory pressure and OOM handling.

Devices with mounts, block holders, writeback, configured secondary compressors,
unreadable memory limits, or initialized non-swap contents are protected. The feature
also blocks when `mmd` is running/restarting, when both `mmd.enabled_aconfig` and
`mmd.zram.enabled` are true, or when property state cannot be obtained. Other vendor
services and root modules cannot be universally identified or excluded. They may
race with or later override a manual configuration.

The transition is `swapoff` (if active), reset, algorithm, size, retained memory
limit, platform mkswap, and swapon. The first failing stage stops further changes.
Zero capacity stops after reset. There are no force flags or blind retries. A
private advisory lock serializes operations from this feature, not external tools.

Each kernel transition runs in a separate process group with a 45-second parent
budget. On timeout, the group is killed and ordinary children are reaped within a
bounded interval. An uninterruptible kernel call may survive userspace termination
and retain the operation lock. Timeout, missing output and contradictory worker
responses mean **outcome unknown**. They are never presented as a rollback or success.
After an error, refresh state before another operation. A reset followed by failure
may leave initialized, inactive swap; that state stays protected because it cannot
be distinguished safely from non-swap contents. Rebooting without a boot plan allows
the system to initialize its own configuration again.

## Saved plans and restoration

State is stored under the existing data root at `var/zram/state.json`. The final
private directory and lock/state files reject symlinks; trusted root-owned ancestor
directories are assumed. Replacements use a synced private temporary file and rename.
There are at most 32 device plans, originals and boot results, with bounded reads.

Saving a plan does not apply it. Boot application is opt-in per device, defaults off,
and runs once after boot completion through the module's existing boot hook. The
boot ID and unknown-result placeholders are persisted before dispatch, preventing
repeated resets after duplicate hooks or interrupted runs. Each plan retains the memory limit observed at application time, and is rechecked
against current capabilities, services and memory headroom. Results remain visible
in the WebUI. Removing a saved plan disables future automatic application, without
changing the current running device. Uninstall removes the saved plans with the data
root; running swap remains until changed by the system or reboot.

Before the first actual operation in a boot, capacity, primary algorithm, priority
(explicit or automatic) and memory limit are saved. Restore only uses an original
from the same boot. It does not recover swapped data, counters, UUID, discard flags,
compressor tuning, writeback, vendor policies, or a byte-for-byte system setup.
Restoring the current device does not disable an independently saved boot plan.

The UI provides 16 locales, isolated LTR technical values, logical RTL alignment,
48 CSS-pixel action targets and reflow checks at 320px with enlarged text. Immediate
before/after allocated-memory readings describe reset effects; they do not establish
performance, battery, or application-retention improvements.

## Validation

Reproducible checks include:

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo check -p duckd --no-default-features --features zram --locked
cargo check -p duckd --target aarch64-linux-android --locked
cargo xtask line-limit
cargo build -p duckd --locked
pnpm --dir ui typecheck
pnpm --dir ui lint
pnpm --dir ui format:check
pnpm --dir ui test
pnpm --dir ui build
pnpm --dir ui exec playwright test
```

Android cross-checks require the configured Android NDK C compiler, archive tool and
linker. Rust tests cover parser bounds, stale state, protected devices, headroom,
first-failure transition ordering, confirmation transport, private storage and worker
protocol/timeouts. API contract tests use the real host binary and an isolated
sysroot. Browser tests use a mock root host. Neither suite performs actual kernel
swap mutations. Device-specific swapoff behavior, SELinux access, vendor races,
real 4/16 KiB swap formatting and boot application still require rooted-device testing.
