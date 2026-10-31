# Device Faker compatibility and validation

## Property memory

Reference baseline: AOSP Android 16 release, bionic commit
[`09a271af557444c9a6b3f3146d6d474156fd6cdb`](https://android.googlesource.com/platform/bionic/+/09a271af557444c9a6b3f3146d6d474156fd6cdb/libc/system_properties/system_properties.cpp).
Bionic retains pointers into property areas; cloned areas therefore remain mapped
for process lifetime. The scanner checks readable code, complete writable context
arrays, bounded SELinux names and readable, complete property mapping starts.
Unreadable, reversed, oversized and nonzero-offset mappings are rejected before copying. Unknown layouts
skip property-memory mutation. A maps snapshot cannot protect against unrelated
concurrent unmapping; runtime discovery is still vendor/version sensitive.

Private copies also validate the complete bounded trie, record offsets, names,
inline/long payload bounds and clean property serials. At most three attempts
bracket a second copy with source allocation/serial checks; a changing or dirty
area is skipped before bionic receives its address. Waiting on a copied dirty bit
would have no writer to wake it. This follows the dirty backup and acquire/release
protocol in [bionic property reads and updates](https://android.googlesource.com/platform/bionic/+/09a271af557444c9a6b3f3146d6d474156fd6cdb/libc/system_properties/system_properties.cpp).
Bit 16 is interpreted as the long-value flag only for `ro.*`, matching that read
path; it remains part of the counter for mutable properties. The pinned property
library still performs property access and mutation. A local nonblocking snapshot
guard is necessary because its public API does not enumerate whole-area serial
offsets without reading/waiting on values. A private area freezes its complete
snapshot, including unrelated values in the same area; it does not track later
global updates. Synthetic areas are protected read-only before attachment. Clone
sealing checks protection failure and never reopens writable memory just to retain
a cosmetic anonymous mapping name.

Linux Android common kernel baseline
[`bc537c78955c9e22326fa13e2982b9e06a3d9f23`](https://android.googlesource.com/kernel/common/+/bc537c78955c9e22326fa13e2982b9e06a3d9f23/mm/mmap.c)
provides the VMA lifecycle reference. Owned private clones replace neither bionic's
real mappings nor another object's Rust mapping owner. `hide_maps` is ignored.
No kernel ABI or private bionic structure layout is assumed universally stable.
Platform command cleanup also follows [Android common kernel child reaping semantics](https://android.googlesource.com/kernel/common/+/bc537c78955c9e22326fa13e2982b9e06a3d9f23/kernel/exit.c):
a failed wait can mean PID ownership was lost (for example, automatic SIGCHLD
reaping). That path returns an error without signalling the numeric PID/group.
Both the native platform adapter and management process boundary use `waitid`
with `WNOWAIT` to retain a completed child's identity until its process group has
been terminated and its status reaped. Normal completion also stops leftover descendants.
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

Global property profiles accept only `ro.*` keys, including deletion requests.
The pinned [mutable update implementation](https://github.com/KernelSU2/ksu_props/blob/ddb6ee7294467f7f25bad2118e9e24eee104144b/crates/prop-rs-android/src/mmap_prop_area.rs)
sets the dirty bit without maintaining the dirty backup used by current AOSP
readers, and restores the previous serial counter after publishing. That protocol
cannot guarantee safe concurrent mutable reads. Both shared configuration parsing
and the global mutation adapter reject these keys before writing. Mutable keys
remain available in private COW mode, with the existing 91-byte payload bound;
private updates occur during application specialization. No replacement global
property writer or new dependency is introduced. Changing read-only properties
through root-only direct writes still falls outside init's ordinary immutable
property contract and is not an atomic transaction across keys or external writers.

## Boot and mutation order

AOSP init/property reference:
[`68be0c2c0006a0740d0b1809abe4717308f90d15`](https://android.googlesource.com/platform/system/core/+/68be0c2c0006a0740d0b1809abe4717308f90d15/init/property_service.cpp),
[system property guidance](https://source.android.com/docs/core/architecture/configuration/add-system-properties).
The toolbox waits for boot completion and finishes legacy property and PI late
stages under an exclusive boot lock before publishing a boot/build-specific
readiness token. Duplicate calls skip global writes once that token is valid.
The compatibility command `stage ready` enters this same ordered boot transaction;
it cannot publish readiness directly or bypass property stages and their lock.
Every legacy property read/write failure contributes to the stage result; later
success cannot hide an earlier failure. Property reads use Android `getprop`, whose
empty default permits absent OEM keys; `resetprop` returns failure for missing keys
and is reserved for writes in this pass. The host command fixture reproduces that
missing-key exit status and runs the actual management boot transaction.
Failure records startup failure and a bounded, boot-scoped cause for the status
response and WebUI details, leaving the gate closed. Stop-token acknowledgement prevents a
stale enable from reopening the gate. Sessions and individual property/DPI writes
retain their enable generation: a rapid stop/re-enable cancels the old generation.
Standalone module conflicts require reboot. A pending toolbox update closes both
the native activation gate and management enable operation until reboot. Empty
enable/stop acknowledgements never count as enabled; stop can replace corrupt tokens.

Global-property cleanup tracks attempted writes and compares the current value
with the applied value before
restoring. This avoids overwriting a later external edit, but global-property
backup is companion-process state: companion crashes and power loss can lose that
backup. Before the first global-property write of a foreground session, a private atomic `global-active` marker
records boot ID, companion PID/start time and enable generation. A missing/dead,
zombie or reused owner blocks further injection and enabling; it does not create
a replacement backup from the remaining spoofed values. The marker is removed only
after successful cleanup by that owner, or during initialization of a new boot.
Same-boot unfinished ownership blocks late initialization before property writes.
Corrupt markers fail closed and are retained for manual inspection. The marker
is ownership evidence, not a persistent copy of the original property values.
A reboot may be required. Cleanup failure markers identify their boot; a
successful boot pass clears an older boot’s global marker after density recovery.
Density recovery requires a readback for the exact recorded user. A mismatched
user leaves the journal intact and performs no restore write. The pre-mutation
baseline must fit the helper’s positive Java integer range; it is checked before
publishing the journal or changing density. Density has a durable private recovery journal,
independent of companion memory, and retains the journal if recovery fails.
The current user must also match the profile-bound user before publishing a new
density journal. A user switch between the focus observation and baseline read
therefore cannot apply that profile to the newly active user. Writes and recovery
continue to address the recorded user explicitly.

The density helper uses explicit user IDs, following the window manager interface
at AOSP frameworks/base
[`99b01a65cc4c104933788b3143285ab6bae65827`](https://android.googlesource.com/platform/frameworks/base/+/99b01a65cc4c104933788b3143285ab6bae65827/services/core/java/com/android/server/wm/WindowManagerShellCommand.java).
Package/user, PID start time and confirmed focused window gate global activation.
One session monitor owns foreground observations. Session state and mutations are
serialized by their ownership mutex; Binder callbacks only send bounded,
coalesced notifications. Periodic checks remain active if observer registration
fails. Queries are rate limited to at most one every 200 ms, with a normal 500 ms
period. This is a scheduling bound, not a device performance measurement.
The registry retains up to 256 live process-bound targets, deduplicates identical
registrations and removes dead or stopped generations after recovery. Registering
a background target preserves the active owner. Switching from A to B restores A
before applying B; returning to the still-live A can activate it without another
Zygisk callback. Unknown focus has a two-second grace period before restoration.
Failed restoration retains the owner and blocks another activation.
Window dumps are filtered before the 64 KiB output limit is applied. On the
[AOSP display dump layout](https://android.googlesource.com/platform/frameworks/base/+/99b01a65cc4c104933788b3143285ab6bae65827/services/core/java/com/android/server/wm/DisplayContent.java),
only display 0 can authorize the global profile because the density helper targets
display 0. Unscoped conflicting focuses, missing users and null focus remain unknown;
a previous focused activity does not override an explicit null current focus.
The Android helper preserves positive integer overrides without an arbitrary
upper threshold, and normalizes the zero/unset sentinel to reset. These semantics
follow the [AOSP window manager implementation](https://android.googlesource.com/platform/frameworks/base/+/99b01a65cc4c104933788b3143285ab6bae65827/services/core/java/com/android/server/wm/WindowManagerService.java).
An unstartable management executable (exit 126/127) releases late property work to
the legacy/PI path. A normal failed boot transaction does not trigger a second
unowned pass. Uninstall reports a pending global cleanup as failure so its private
recovery data is retained.

TOML is parsed and checked for embedded NUL characters in keys and values before
creating a native property plan. This also checks unused templates; no truncated
C string may be published through a configuration save. Custom property names and
mutable value lengths follow [AOSP init validation](https://android.googlesource.com/platform/system/core/+/68be0c2c0006a0740d0b1809abe4717308f90d15/init/util.cpp).
Non-`ro.*` values must fit the 91-byte inline payload; allowing long-value records
would conflict with bionic's mutable read path. Read-only properties retain long-value support.
SDK integer and DPI limits are checked by the shared parser for every direct app
and template, including unused or shadowed records. Invalid values are rejected
before JNI fields are modified; the native path cannot silently discard an invalid
density while the management path rejects it. The positive Java integer SDK bound
follows the [JNI primitive type contract](https://docs.oracle.com/en/java/javase/21/docs/specs/jni/types.html).

Commands and frames have deadlines; stop closes new injection immediately, while
cleanup can take longer and reports pending failures.

## Zygisk integration

The public [Zygisk API v4 header](https://github.com/topjohnwu/zygisk-module-sample/blob/7bb941ac8edfcffd1d23761e401c45ca95409dc1/module/jni/zygisk.hpp)
is the ABI contract. The composite callback table registers once and dispatches PI
before Device Faker, deferring library unload until callbacks finish. PI-selected
processes receive no Device Faker callback. Companion routing validates a versioned
frame and owns descriptors explicitly. Original header copyright is retained.
Package extraction falls back to the process name when the data-directory Java
reference is null, and removes the process suffix before matching a profile.
The enable generation is rechecked after configuration resolution and before hooks.

[ZygiskNext documentation](https://github.com/LSPosed/ZygiskNext/tree/db994f8caebd440eedd02beab38ca62f3d1f8a2a)
distinguishes ordinary Zygisk application callbacks from its init-oriented API and
HyperOS native-only callbacks. The embedded engine requires ordinary Android
ART/JNI Zygisk callbacks; the native-only HyperOS runtime is not a supported target.
No proprietary ZygiskNext implementation is bundled or modified.

Pre-specialization diagnostics remain in a bounded process-local buffer until
the companion connection is available. It retains at most 24 KiB and 256 recent
lines, truncates each line at a UTF-8 boundary within 2 KiB, and reports how many
older lines were dropped. The bound leaves room for worst-case JSON escaping in
the 256 KiB companion frame. Draining resets the buffer; excessive debug output
cannot cause the entire diagnostic frame to exceed that transport limit. This
does not impose a rotation policy on the companion's on-disk log file.

Build field lookup follows [Android JNI guidance](https://developer.android.com/ndk/guides/jni-tips)
and the JNI library’s typed lookup errors. Missing fields are skipped individually;
other lookup or initialization failures abort the hook. For example,
[`Build.SOC_MODEL` starts at API 31](https://developer.android.com/reference/android/os/Build#SOC_MODEL).
A missing field on older Android must not prevent subsequent fingerprint/version
updates. An exception already pending on entry is preserved. A host JVM fixture
exercises the actual Rust JNI hook against a class without `SOC_MODEL`; it is not
an ART or OEM compatibility test. An explicit SDK override updates both `SDK_INT`
and the deprecated string `SDK`, whose shared source is shown in
[AOSP Build](https://android.googlesource.com/platform/frameworks/base/+/99b01a65cc4c104933788b3143285ab6bae65827/core/java/android/os/Build.java).

## Interface and accessibility

The interface applies Material 3 Expressive hierarchy and tonal surfaces while
following [Android accessibility](https://developer.android.com/design/ui/mobile/guides/foundations/accessibility)
and [Apple accessibility principles](https://developer.apple.com/design/human-interface-guidelines/accessibility).
[Google's Material 3 Expressive overview](https://blog.google/products-and-platforms/platforms/android/material-3-expressive-android-wearos-launch/)
and [Material design research](https://design.google/library/expressive-material-design-google-research)
are the visual references. Section headings and field surfaces share a leading
alignment, following [Apple layout guidance](https://developer.apple.com/design/human-interface-guidelines/layout).
Controls use the shared design tokens, explicit labels,
visible state text and 48 px minimum touch targets. Editing, preview, apply and
power state are separate. Unsaved drafts persist across navigation. The editor caps manual input length
and retains a bounded draft even when multibyte text exceeds the 48 KiB publication
limit. It shows a localized error linked to the text area; oversized text cannot
be previewed/applied. Identical cached/saved content uses the current revision. Source code
and TOML remain left-to-right within translated or right-to-left layouts. Counts
use locale-aware number formatting. Startup failure suppresses the generic waiting
notice; an armed but blocked engine reports unmet conditions rather than claiming injection.
Preview requests retain a generation invalidated by edits or page disposal, including
edits that return to the original text. A successful stop updates cached power state
before readback, so a failed status refresh cannot make the engine appear enabled again.
Enable acknowledgements also update cached state before readback, following
[TanStack mutation-response guidance](https://tanstack.com/query/latest/docs/framework/vue/guides/updates-from-mutation-responses).
A later stop or page disposal invalidates pending enable responses and errors.
A configuration-save acknowledgement updates the shared saved snapshot, preserves
edits made while saving, and cannot clear another page's stored draft after the
submitting page has closed. Retained edits use the new saved revision as their
base. Invalid saved configuration exposes its technical parse details alongside
the localized error, following [Apple feedback guidance](https://developer.apple.com/design/human-interface-guidelines/feedback).
The property-mode restriction is explained in all sixteen locale dictionaries and
is programmatically associated with the TOML field. Technical diagnostics remain
left-to-right inside RTL pages; their disclosure is keyboard accessible.
Runtime template names use bidirectional isolation and localized list separators,
following [W3C inline bidirectional text guidance](https://www.w3.org/International/articles/inline-bidi-markup/).
Template-name input determines its direction from the entered text. Confirmation
dialogs preserve intrinsic row heights and scroll rather than compressing the
description into their actions at large text sizes.

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
both management CLIs, licenses, hashes and corresponding-source links pinned to
the build commit. Device Faker remains an ordinary source component statically
linked into that module, with no Git submodule or second installable root module.
Packaging rejects modified source checkouts and gitlinks, verifies artifact hashes
and notices, preserves executable bits, and publishes the ZIP atomically after CRC validation.
Host tests, ABI harnesses, source inspection and Android cross-compilation do not
prove real injection, every OEM bionic layout, JNI field availability or actual
property/density restoration on a device. `injection_verified` remains false.
