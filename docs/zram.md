# ZRAM and swap

The `zram` feature manages existing ZRAM block devices used as swap. Other swap
partitions and files are listed read-only. It neither creates swap files nor edits
fstab, init scripts, Android properties, SELinux policy, or memory-manager settings.

## Source baseline

Implementation references (WebUI/backend feature contract 3):

- [ACK android15-6.6 ZRAM driver](https://android.googlesource.com/kernel/common/+/aa8f225d2792f5fd45e3a853fc2fd5d2e7baf8af/drivers/block/zram/zram_drv.c):
  `comp_algorithm`, initialization, memory counters, secondary compressors, writeback,
  and `reset_store()` refusing open or claimed devices.
  `init_done()` returns a boolean derived from `disksize`; standard `initstate`
  output is `0` or `1`.
- [ACK ZRAM sysfs ABI](https://android.googlesource.com/kernel/common/+/aa8f225d2792f5fd45e3a853fc2fd5d2e7baf8af/Documentation/ABI/testing/sysfs-block-zram)
  and [upstream ZRAM documentation](https://docs.kernel.org/admin-guide/blockdev/zram.html):
  counter order and units, selected algorithm syntax, reset semantics and swap setup.
  [ACK block attributes](https://android.googlesource.com/kernel/common/+/aa8f225d2792f5fd45e3a853fc2fd5d2e7baf8af/block/genhd.c)
  define the `ro` read-only state used to protect devices before deactivation.
- [ACK block-device file operations](https://android.googlesource.com/kernel/common/+/aa8f225d2792f5fd45e3a853fc2fd5d2e7baf8af/block/fops.c)
  and [claim handling](https://android.googlesource.com/kernel/common/+/aa8f225d2792f5fd45e3a853fc2fd5d2e7baf8af/block/bdev.c):
  an `O_EXCL` block-device open holds a claim until its open file description is released.
  The ZRAM driver refuses hot removal while there are openers.
- [ACK disk sequence ABI](https://android.googlesource.com/kernel/common/+/aa8f225d2792f5fd45e3a853fc2fd5d2e7baf8af/Documentation/ABI/stable/sysfs-block):
  device sequence numbers distinguish a reused device name from an earlier instance.
- [ACK swap implementation](https://android.googlesource.com/kernel/common/+/aa8f225d2792f5fd45e3a853fc2fd5d2e7baf8af/mm/swapfile.c):
  permission checks, swapoff memory/error handling, automatic and explicit priorities.
- [AArch64 generic syscall numbers](https://android.googlesource.com/kernel/common/+/aa8f225d2792f5fd45e3a853fc2fd5d2e7baf8af/include/uapi/asm-generic/unistd.h):
  `swapon=224`, `swapoff=225`, and worker supervision `waitid=95`.
  [ARM EABI](https://android.googlesource.com/kernel/common/+/aa8f225d2792f5fd45e3a853fc2fd5d2e7baf8af/arch/arm/tools/syscall.tbl)
  uses different numbers (`87`, `115`, `280`). Mutations are enabled only on Android ARM64,
  use target libc wrappers and assert the ARM64 constants at compilation. Host and
  ARM32 builds can browse but cannot enter the mutation path.
- [AOSP fs_mgr](https://android.googlesource.com/platform/system/core/+/a3b721a32242006b59cb12bd62c9133632af3a2d/fs_mgr/fs_mgr.cpp):
  ZRAM setup, `/system/bin/mkswap`, priority flags and the transition to `mmd`.
- [Bionic swap declarations](https://android.googlesource.com/platform/bionic/+/731631f300090436d7f5df80d50b6275c8c60a93/libc/include/sys/swap.h)
  and [system property API](https://android.googlesource.com/platform/bionic/+/731631f300090436d7f5df80d50b6275c8c60a93/libc/include/sys/system_properties.h):
  libc signatures, flag encoding and bounded property-area reads.
- [AOSP boolean parsing](https://android.googlesource.com/platform/system/libbase/+/6d19b5c690fa4220c10e42c2326150bbd4d4b7bb/parsebool.cpp):
  supported spellings for boolean properties, including `1`, `yes` and `on`.
- [waitid semantics](https://man7.org/linux/man-pages/man2/waitpid.2.html):
  `WNOWAIT` observes exit while retaining the leader PID until group cleanup.
- [AOSP Toybox mkswap](https://android.googlesource.com/platform/external/toybox/+/59a869a838cfadbf6bc41cb400471ca9d2cae9b5/toys/other/mkswap.c):
  swap header formatting with the runtime page size and an unsigned 32-bit last-page
  field. Before deactivation, positive capacity must contain a header and data page,
  fit that field, and align to the running page size; the retained memory limit must
  also be page aligned. The platform formatter is used directly without a shell.
  This feature does not implement its own swap format.
- [Material 3 dialogs](https://m3.material.io/components/dialogs/guidelines),
  [Android dialog guidance](https://developer.android.com/develop/ui/compose/components/dialog),
  [Apple alerts](https://developer.apple.com/design/human-interface-guidelines/alerts),
  [Apple accessibility](https://developer.apple.com/design/human-interface-guidelines/accessibility):
  explicit action labels, confirmation for resetting swap, persistent in-context
  feedback, readable layouts and keyboard focus restoration. The automatic-priority
  control follows [Android switch semantics](https://developer.android.com/develop/ui/compose/components/switch).
- [ECMAScript safe integers](https://tc39.es/ecma262/multipage/numbers-and-dates.html#sec-number.max_safe_integer):
  the WebUI rejects negative, fractional or unsafe integer byte/page fields, including
  values above `Number.MAX_SAFE_INTEGER`. Native CLI and worker transport retain u64
  values. An unrepresentable UI value is a contract error, never a rounded edit.

## Operation contract

`duckd zram status` returns physical/available memory, global `pswpin`/`pswpout`
page counts, `/proc/swaps`, ZRAM device identity, algorithm capabilities and memory
statistics. Compressed data size and allocated physical memory are separate values.
The compressed/original ratio is absent for an empty device. Page counts use the
running kernel's page size, not an assumed 4096 bytes.

An active swap device is associated with ZRAM using block-device major/minor identity,
including aliased paths. An unresolved active partition blocks resets. The revision
also identifies the device instance using disk sequence (when available) and sysfs
inode identity. Device identity, instance, size and initialization are rechecked at
the end of each inventory read, and before reset, formatting and activation. A
removed/recreated same-name device is distinguished by disk sequence where supported. The inode
fallback on older kernels is best effort and cannot guarantee detection of inode
reuse; avoid removing and recreating a device between changes and restoration.
Legacy originals without an instance identifier are not restored; the next actual
change captures a new original. Missing or malformed required state fails closed.
Missing optional statistics remain unknown. The stable configuration fields are
read again before publishing an inventory: read-only state, primary algorithm,
backing device, secondary compressors and memory limit must still match.

Some vendor kernels expose capacity through `initstate` instead of normalizing it
to a boolean. Compatibility accepts the full capacity or its low 32 bits, only
when they match the separately read `disksize`. For example, capacity `8053063680`
and initialization value `3758096384` describe an initialized 7.5 GiB device.
Capacity multiples of 4 GiB can produce a truncated zero; these remain initialized
when the nonzero capacity matches this encoding. This is a compatibility rule,
not an upstream ABI guarantee. Unrelated positive numbers, malformed values and
conflicting zero/size pairs are not treated as a generic truthy value.

Unreadable, unrecognized or inconsistent initialization is represented as `null`,
with the device still listed and protected by `initstate_unknown`. The WebUI/backend
contract is 3 because initialization was previously a required boolean. Unknown
initialization cannot authorize a transition or verify successful completion.
The first four `mm_stat` fields are decoded independently; unused extension fields
do not invalidate usable counters. Missing or malformed memory limits block changes
without removing the device. Diagnostics distinguish incomplete discovery from an
empty inventory and preserve readable technical details separately from localized
feedback.

Runtime operations require Android ARM64, effective UID 0, the real `/` sysroot,
explicit confirmation and a configuration revision from the current boot. Input
rejects unknown fields. The UI and backend both validate size and priority; the
backend additionally validates the running page alignment and device identity.
The editor accepts whole MiB from 16 MiB to twice physical memory, or zero to disable.
This upper limit is application policy, not a kernel ABI limit. Existing non-MiB
capacities are displayed without silent rounding; editing requires an explicit whole
MiB choice. Arabic, Persian and full-width decimal digits and the Unicode minus sign
are accepted. Input errors are associated with their own fields. Confirmation shows
exact capacity in MiB and the exact nonzero memory limit in bytes. Restoration retains
the original byte size even when it is outside this editing policy.

Before deactivation, available memory must exceed used swap bytes plus the larger
of 256 MiB or 10% of physical memory. This conservative check cannot guarantee
successful swapoff or prevent concurrent memory pressure and OOM handling.

Devices marked read-only, with mounts, block holders, writeback, configured secondary
compressors, unreadable memory limits, or initialized non-swap contents are protected.
The feature also blocks when `mmd` is running/restarting/stopping, when
`mmd.enabled_aconfig` and `mmd.zram.enabled` enable management (including AOSP boolean
spellings), or when property state cannot be obtained. Other vendor
services and root modules cannot be universally identified or excluded. They may
race with or later override a manual configuration.

The transition is `swapoff` (if active), reset, algorithm, size, retained memory
limit, platform mkswap, and swapon. The first failing stage stops further changes.
Formatting obtains an exclusive read/write block descriptor, verifies its identity and device instance, and gives
that descriptor to the platform formatter as standard input with `/proc/self/fd/0`
as its target. This pins the ZRAM instance and excludes competing filesystem/swap
claims during header creation. The claim is released before activation; ordinary
writes by other privileged processes cannot be universally excluded.
Zero capacity resets and restores the chosen algorithm and memory limit without
formatting or activation. This also preserves a previously uninitialized device's
basic configuration. Unchanged configurations do not reset, require a formatter,
check swapoff headroom, or create an original record. There are no force flags or
blind retries. A private advisory lock serializes operations from this feature,
not external tools.

Each kernel transition runs in a separate process group with a 45-second parent
budget. Exit is observed with `waitid(WNOWAIT)`; the process group is terminated before
reaping the leader, preventing descendants from surviving a completed worker and
avoiding reuse of the leader PID during cleanup. On timeout the group is killed and
ordinary children are reaped within a bounded interval. An uninterruptible kernel call
may survive userspace termination and retain a device claim or the operation lock.
Timeout, missing output and contradictory worker responses mean **outcome unknown**.
They are never presented as a rollback or success.
After an error, refresh state before another operation. A reset followed by failure
may leave initialized, inactive swap; that state stays protected because it cannot
be distinguished safely from non-swap contents. Rebooting without a boot plan allows
the system to initialize its own configuration again.

## Saved plans and restoration

State is stored under the existing data root at `var/zram/state.json`. The final
private directory and lock/state files reject symlinks; trusted root-owned ancestor
directories are assumed. Replacements use a synced private temporary file and rename.
There are at most 32 device plans, originals, boot claims and boot results, with
bounded reads and validated identities. Status reads the atomically replaced state
without waiting for a mutation lock and reports that lock as busy. The WebUI keeps
inspection and refresh available while disabling further changes.

Saving a plan does not apply it. Boot application is opt-in per device, defaults off,
and runs once after boot completion through the module's existing boot hook. The
boot ID and unknown-result placeholders are persisted before dispatch, preventing
repeated resets after duplicate hooks or interrupted runs. Each worker rechecks the
saved plan and atomically consumes its per-boot authorization under the mutation
lock before acting. Every save receives a new per-device version from a persistent
monotonic counter. Disabling, changing, removing or replacing an identical plan
invalidates an unclaimed older request; changing a different device's plan leaves
this device's version intact. Legacy saved plans use version zero until first saved.
Each plan retains the memory limit observed at application time, and is rechecked
against current capabilities, services and memory headroom. Results remain visible
in the WebUI. Removing a saved plan disables future automatic application, without
changing the current running device. Plans whose devices are currently unavailable
remain visible with a removal action, so a missing device cannot trap an enabled
boot plan. Uninstall removes the saved plans with the data root; running swap remains
until changed by the system or reboot.

Before the first actual operation in a boot, capacity, primary algorithm, priority
(explicit or automatic) and memory limit are saved. Restore only uses an original
from the same boot and device instance. It does not recover swapped data, counters,
UUID, discard flags, compressor tuning, writeback, vendor policies, or a byte-for-byte
system setup.
Restoring the current device does not disable an independently saved boot plan.

The editor retains drafts during ordinary refreshes and resets them when a device
instance changes. An automatic-priority switch avoids requiring a minus key on mobile
keyboards and preserves the previous explicit value when toggled off. The numeric
field remains read-only in automatic mode, preserving focus for typed `-1` input.

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
first-failure transition ordering, read-only protection, swap-header geometry,
disabled-state restoration, no-op behavior, confirmation transport, atomic boot
authorization, snapshots under lock, private storage, saved-plan replacement,
vendor initialization encodings (including zero after 32-bit truncation),
unknown-initialization protection and independent optional statistics,
worker protocol consistency, process-group cleanup and timeouts. API contract tests
use the real host binary and an isolated sysroot, including integer-precision
rejection. Browser tests use a mock root host and cover draft lifetime, unavailable
plans, focus and layout. These checks do not perform actual kernel swap mutations.
Device-specific swapoff behavior, SELinux access, vendor races,
real 4/16 KiB swap formatting and boot application still require rooted-device testing.
