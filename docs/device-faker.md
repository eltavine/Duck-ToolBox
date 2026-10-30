# Device Faker compatibility and validation

## Property memory

Reference baseline: AOSP Android 16 release, bionic commit
[`09a271af557444c9a6b3f3146d6d474156fd6cdb`](https://android.googlesource.com/platform/bionic/+/09a271af557444c9a6b3f3146d6d474156fd6cdb/libc/system_properties/system_properties.cpp).
Bionic retains pointers into property areas; cloned areas therefore remain mapped
for process lifetime. The scanner checks readable code, complete writable context
arrays, bounded SELinux names and actual property mapping starts. Unknown layouts
skip property-memory mutation. A maps snapshot cannot protect against unrelated
concurrent unmapping; runtime discovery is still vendor/version sensitive.

Linux Android common kernel baseline
[`bc537c78955c9e22326fa13e2982b9e06a3d9f23`](https://android.googlesource.com/kernel/common/+/bc537c78955c9e22326fa13e2982b9e06a3d9f23/mm/mmap.c)
provides the VMA lifecycle reference. Owned private clones replace neither bionic's
real mappings nor another object's Rust mapping owner. `hide_maps` is ignored.
No kernel ABI or private bionic structure layout is assumed universally stable.
Platform command cleanup also follows [Android common kernel child reaping semantics](https://android.googlesource.com/kernel/common/+/bc537c78955c9e22326fa13e2982b9e06a3d9f23/kernel/exit.c):
a failed wait can mean PID ownership was lost (for example, automatic SIGCHLD
reaping). That path returns an error without signalling the numeric PID/group.
Density transaction guards explicitly unlock before closing their descriptor,
consistent with [kernel file-lock lifetime](https://android.googlesource.com/kernel/common/+/bc537c78955c9e22326fa13e2982b9e06a3d9f23/fs/locks.c).
A forked child retaining a descriptor must not extend a completed transaction.
An isolated host fixture verifies reacquisition while that child is still alive.

Global areas are never compacted during session application or restoration.
The pinned [property library rebuild implementation](https://github.com/KernelSU2/ksu_props/blob/ddb6ee7294467f7f25bad2118e9e24eee104144b/crates/prop-rs-android/src/sys_prop.rs)
recreates entries and copies the new area over the old area; cached `prop_info*`
pointers could then refer to relocated or unrelated entries. Long-value orphan
buffers are retained rather than reclaiming them through live-area reordering.
The [AOSP ContextNode definition](https://android.googlesource.com/platform/bionic/+/09a271af557444c9a6b3f3146d6d474156fd6cdb/libc/system_properties/include/system_properties/context_node.h)
is the layout reference, with unknown layouts still rejected conservatively.

## Boot and mutation order

AOSP init/property reference:
[`68be0c2c0006a0740d0b1809abe4717308f90d15`](https://android.googlesource.com/platform/system/core/+/68be0c2c0006a0740d0b1809abe4717308f90d15/init/property_service.cpp),
[system property guidance](https://source.android.com/docs/core/architecture/configuration/add-system-properties).
The toolbox waits for boot completion and finishes legacy property and PI late
stages under an exclusive boot lock before publishing a boot/build-specific
readiness token. Duplicate calls skip global writes once that token is valid.
Every legacy property read/write failure contributes to the stage result; later
success cannot hide an earlier failure. Failure records
startup failure and leaves the gate closed. Stop-token acknowledgement prevents a
stale enable from reopening the gate. Sessions and individual property/DPI writes
retain their enable generation: a rapid stop/re-enable cancels the old generation.
Standalone module conflicts require reboot.

Global-property cleanup tracks attempted writes and compares the current value
with the applied value before
restoring. This avoids overwriting a later external edit, but global-property
backup is companion-process state: companion crashes and power loss can lose that
backup. A reboot may be required. Cleanup failure markers identify their boot; a
successful boot pass clears an older boot’s global marker after density recovery.
Density recovery requires a readback for the exact recorded user. A mismatched
user leaves the journal intact and performs no restore write. The pre-mutation
baseline must fit the helper’s positive Java integer range; it is checked before
publishing the journal or changing density. Density has a durable private recovery journal,
independent of companion memory, and retains the journal if recovery fails.

The density helper uses explicit user IDs, following the window manager interface
at AOSP frameworks/base
[`99b01a65cc4c104933788b3143285ab6bae65827`](https://android.googlesource.com/platform/frameworks/base/+/99b01a65cc4c104933788b3143285ab6bae65827/services/core/java/com/android/server/wm/WindowManagerShellCommand.java).
Package/user, PID start time and confirmed focused window gate global activation.
The Android helper preserves positive integer overrides without an arbitrary
upper threshold, and normalizes the zero/unset sentinel to reset. These semantics
follow the [AOSP window manager implementation](https://android.googlesource.com/platform/frameworks/base/+/99b01a65cc4c104933788b3143285ab6bae65827/services/core/java/com/android/server/wm/WindowManagerService.java).
An unstartable management executable (exit 126/127) releases late property work to
the legacy/PI path. A normal failed boot transaction does not trigger a second
unowned pass. Uninstall reports a pending global cleanup as failure so its private
recovery data is retained.

TOML is parsed and checked for embedded NUL characters in keys and values before
creating a native property plan. This also checks unused templates; no truncated
C string may be published through a configuration save.

Commands and frames have deadlines; stop closes new injection immediately, while
cleanup can take longer and reports pending failures.

## Zygisk integration

The public [Zygisk API v4 header](https://github.com/topjohnwu/zygisk-module-sample/blob/7bb941ac8edfcffd1d23761e401c45ca95409dc1/module/jni/zygisk.hpp)
is the ABI contract. The composite callback table registers once and dispatches PI
before Device Faker, deferring library unload until callbacks finish. PI-selected
processes receive no Device Faker callback. Companion routing validates a versioned
frame and owns descriptors explicitly. Original header copyright is retained.

[ZygiskNext documentation](https://github.com/LSPosed/ZygiskNext/tree/db994f8caebd440eedd02beab38ca62f3d1f8a2a)
distinguishes ordinary Zygisk application callbacks from its init-oriented API and
HyperOS native-only callbacks. The embedded engine requires ordinary Android
ART/JNI Zygisk callbacks; the native-only HyperOS runtime is not a supported target.
No proprietary ZygiskNext implementation is bundled or modified.

Build field lookup follows [Android JNI guidance](https://developer.android.com/ndk/guides/jni-tips)
and the JNI library’s typed lookup errors. Missing fields are skipped individually;
other lookup or initialization failures abort the hook. For example,
[`Build.SOC_MODEL` starts at API 31](https://developer.android.com/reference/android/os/Build#SOC_MODEL).
A missing field on older Android must not prevent subsequent fingerprint/version
updates. An exception already pending on entry is preserved. A host JVM fixture
exercises the actual Rust JNI hook against a class without `SOC_MODEL`; it is not
an ART or OEM compatibility test.

## Interface and accessibility

The interface applies Material 3 Expressive hierarchy and tonal surfaces while
following [Android accessibility](https://developer.android.com/design/ui/mobile/guides/foundations/accessibility)
and [Apple accessibility principles](https://developer.apple.com/design/human-interface-guidelines/accessibility).
[Google's Material 3 Expressive overview](https://blog.google/products-and-platforms/platforms/android/material-3-expressive-android-wearos-launch/)
is the visual reference. Controls use the shared design tokens, explicit labels,
visible state text and 48 px minimum touch targets. Editing, preview, apply and
power state are separate. Unsaved drafts persist across navigation. The editor caps manual input length
and retains a bounded draft even when multibyte text exceeds the 48 KiB publication
limit. It shows a localized error linked to the text area; oversized text cannot
be previewed/applied. Identical cached/saved content uses the current revision. Source code
and TOML remain left-to-right within translated or right-to-left layouts.

Sixteen locale dictionaries share the same keys. Browser checks cover all locales
at 320 px, light/dark themes, 200% root font size, draft preservation and independent
stop during startup failure. Automated geometry checks do not certify translation
quality, TalkBack behavior or every dynamic device theme; those need device review.

## Reproduction and limits

```sh
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --manifest-path components/device-faker/manager/Cargo.toml --target-dir target/device-faker-manager
python3 scripts/test-device-faker.py
python3 scripts/test-zygisk-runtime.py
pnpm --dir ui test
pnpm --dir ui typecheck
pnpm --dir ui lint
pnpm --dir ui exec playwright test e2e/device-faker.spec.ts
python3 scripts/build-play-integrity.py
```

The installation ZIP contains one root-manager module, one Zygisk library per ABI,
both management CLIs, licenses, hashes and the exact corresponding-source archive.
The archive carries locked Rust dependencies and pinned native build dependencies.
Packaging compares every archived regular source file with the build commit
(except the required Cargo vendor configuration extension), preserves executable
bits, and publishes the final ZIP atomically only after CRC validation.
Host tests, ABI harnesses, source inspection and Android cross-compilation do not
prove real injection, every OEM bionic layout, JNI field availability or actual
property/density restoration on a device. `injection_verified` remains false.
