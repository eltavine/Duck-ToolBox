# Kernel module contracts and verification

## Source baseline

The Android Common Kernel baseline is `android15-6.6` at
`2bdbbebc3bff5804c139a0806aeb96187b3cbb24`. Bionic is pinned to
`731631f300090436d7f5df80d50b6275c8c60a93`, and AOSP system/core to
`a3b721a32242006b59cb12bd62c9133632af3a2d`.

These source revisions establish the contracts below. They do not establish
compatibility with a particular vendor kernel or security policy.

| Contract | Primary reference |
| --- | --- |
| Loaded module rows, missing reference counts, permanent markers and states | [ACK procfs.c](https://android.googlesource.com/kernel/common/+/2bdbbebc3bff5804c139a0806aeb96187b3cbb24/kernel/module/procfs.c) |
| Module permission, image validation, loading and removal checks | [ACK main.c](https://android.googlesource.com/kernel/common/+/2bdbbebc3bff5804c139a0806aeb96187b3cbb24/kernel/module/main.c) |
| Release-prefix comparison when symbol CRCs exist | [ACK version.c](https://android.googlesource.com/kernel/common/+/2bdbbebc3bff5804c139a0806aeb96187b3cbb24/kernel/module/version.c) |
| Parameter getter output and appended newline | [ACK params.c](https://android.googlesource.com/kernel/common/+/2bdbbebc3bff5804c139a0806aeb96187b3cbb24/kernel/params.c) |
| Module name buffer, including its terminating NUL | [ACK module.h](https://android.googlesource.com/kernel/common/+/2bdbbebc3bff5804c139a0806aeb96187b3cbb24/include/linux/module.h) and [moduleparam.h](https://android.googlesource.com/kernel/common/+/2bdbbebc3bff5804c139a0806aeb96187b3cbb24/include/linux/moduleparam.h) |
| Kbuild replacement of hyphens and commas in module names | [ACK Makefile.lib](https://android.googlesource.com/kernel/common/+/2bdbbebc3bff5804c139a0806aeb96187b3cbb24/scripts/Makefile.lib) |
| ARM64 UAPI syscall numbers | [ACK asm-generic/unistd.h](https://android.googlesource.com/kernel/common/+/2bdbbebc3bff5804c139a0806aeb96187b3cbb24/include/uapi/asm-generic/unistd.h) and [ARM64 inclusion](https://android.googlesource.com/kernel/common/+/2bdbbebc3bff5804c139a0806aeb96187b3cbb24/arch/arm64/include/uapi/asm/unistd.h) |
| Bionic register transfer and errno handling | [ARM64 syscall.S](https://android.googlesource.com/platform/bionic/+/731631f300090436d7f5df80d50b6275c8c60a93/libc/arch-arm64/bionic/syscall.S) |
| AOSP file-based module loading and non-forced removal | [libmodprobe_ext.cpp](https://android.googlesource.com/platform/system/core/+/a3b721a32242006b59cb12bd62c9133632af3a2d/libmodprobe/libmodprobe_ext.cpp) |
| GKI, vendor modules and symbol restrictions | [AOSP kernel modules overview](https://source.android.com/docs/core/architecture/kernel/modules) |

## Runtime operations

The feature contract is version 2. Backend and WebUI must be updated together;
the feature registry rejects a version-1 backend before exposing operations.
The shared command-envelope API remains version 1.

Execution is restricted to Android AArch64 root processes using the real `/`
sysroot. The adapter uses libc's target-specific constants. Compile-time assertions
check AArch64 numbers `105` (`init_module`), `106` (`delete_module`) and `273`
(`finit_module`), along with the 64-bit C `long` width. Bionic performs the register
transfer to `x8` and `x0`–`x5` and translates kernel errors into `errno`.

Loading uses `init_module` with the same owned image bytes that were inspected and
hashed. This avoids reopening a mutable path between digest verification and the
syscall. In contrast, AOSP libmodprobe uses `finit_module` on an opened file. These
interfaces have different security-hook paths; a policy can reject the memory-image
operation even when a file-based load is permitted. There is no automatic fallback
to another loading interface after rejection.

Preflight requires a little-endian ARM64 ELF64 image and one nonempty vermagic entry.
The release-prefix comparison is advisory: the kernel can ignore this prefix when
symbol CRCs are available. A matching prefix does not establish ABI, symbol or
signature compatibility. ELF metadata is descriptive, not proof of trusted code.

Removal requests use `O_NONBLOCK`; `O_TRUNC` is never passed. Live state, zero
references and absence of dependents/holders are preliminary checks. The kernel
still checks permissions, references and exit support at the actual syscall.
Missing reference counts (`-`) remain unknown and prevent an unload offer.
Entries visible only in sysfs are not classified as unloadable modules.

The ARM64 name buffer holds 56 bytes including NUL, so mutations require names of
at most 55 bytes. Browsing permits up to 128 bytes for sysfs entries. Supported
name characters are ASCII letters, digits, underscores, hyphens, periods and plus
signs; `.` and `..` are rejected as path components. This is an application input
policy, not a claim that the kernel enforces the same character whitelist.

## Process and file boundaries

Mutations run in an isolated worker with a private input file and private output
file. A file lock serializes operations. Lock contention is distinguished from
other I/O failures. The parent validates envelope version, command, action and the
typed result, rather than accepting an arbitrary success object.
Successful removal results must identify the requested module. Directory permission
changes operate on an opened directory descriptor; the final directory and lock
components reject symlinks, and the lock must be a regular file. Ancestor directories
remain part of the trusted application storage boundary.

The response budget is 30 seconds. Error paths terminate the userspace worker and
attempt bounded reaping. An uninterruptible kernel init/exit routine may continue;
timeout, missing output and abnormal termination report an unknown outcome. No
mutation is retried automatically. Refreshing state does not prove rollback.

Inspection accepts regular, uncompressed `.ko` files up to 64 MiB. The ELF reader
is isolated inside the feature and exposes a stable architecture enum. Invalid
UTF-8 metadata is displayed with replacement characters and a warning; SHA-256
still covers original bytes. Parameter values preserve whitespace and remove only
the one newline appended by the kernel getter. Directory scan limits and partial
read failures are reported without discarding unrelated entries.
Metadata is limited to 1 MiB, 8192 entries and 128 distinct keys of at most 128
bytes each. These are application resource budgets, not ELF or kernel limits;
over-budget input is rejected rather than silently truncated. The metadata view
mounts its contents only when expanded.

## Interface and localization

The interface follows the existing Material 3 Expressive component system and
its logical spacing, rounded surfaces and state layers. Applicable guidance:

- [Material 3 lists](https://m3.material.io/components/lists/overview): consistent
  content/action slots, scannable rows and segmented grouping.
- [Android grids and units](https://developer.android.com/design/ui/mobile/guides/layout-and-content/grids-and-units)
  and [touch targets](https://support.google.com/accessibility/android/answer/7101858):
  scalable layouts and at least 48 CSS-pixel touch regions in this WebUI.
- [Apple accessibility](https://developer.apple.com/design/human-interface-guidelines/accessibility)
  and [layout](https://developer.apple.com/design/human-interface-guidelines/layout):
  legibility, focus, feedback, adaptation to larger text and reachable controls.
- [Apple right-to-left guidance](https://developer.apple.com/design/human-interface-guidelines/right-to-left):
  logical row alignment with separately isolated technical strings.
- [WCAG text contrast](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html):
  at least 4.5:1 for ordinary text, including active destructive labels and
  confirmation controls in both themes.

The feature provides 16 locales. Runtime states, empty states, action labels and
metadata labels are translated; numbers follow the selected locale. Paths, module
names and hashes retain their character direction without forcing the whole row
to left alignment. Message keys, nonempty translations and named placeholders are
checked across locales. File inspection supports Enter from the path field.
Cancel restores the initiating control's focus; accepted operations move focus to
the active tab because mutations can disable or remove the initiating control.

## Reproducible checks and limits

From the repository root:

```sh
cargo test -p duck-kernel-modules --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo check -p duck-kernel-modules --locked --target aarch64-linux-android
cargo check -p duckd --locked --no-default-features --features kernel-modules
cargo xtask line-limit
```

From `ui/`, after building the host `duckd` binary:

```sh
pnpm test
pnpm typecheck
pnpm lint
pnpm format:check
pnpm exec playwright test e2e/kernel-modules.spec.ts
pnpm exec playwright test e2e/layout.spec.ts e2e/screenshots.spec.ts
```

Coverage includes procfs variants, byte order/class checks, changed files,
parameter whitespace, scan limits, worker protocol failures, bounded timeout
cleanup, confirmation, pending/error feedback, keyboard focus, draft preservation,
200% text, narrow screens and RTL alignment. Browser tests use a simulated root
host. Cross-compilation and host tests do not verify real-device module loading,
unloading, module initialization or vendor LSM/signature acceptance.
