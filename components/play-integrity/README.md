# Play Integrity Engine

A GPLv3 source component embedded in Duck ToolBox, derived from Play Integrity
Fork at revision `23ec56c2440c4fd1a5e893a23ef7861d6366a828`.
See NOTICE, LICENSE and UPSTREAM.json. It is a regular source directory, with no
Git submodule or separate root-manager installation. Its CLI remains the process
boundary for the toolbox adapters and boot hooks, including direct shell use.

## Build and installation

Requires Python 3.12+, Rust 1.89+, JDK 21, Android SDK 37, Build Tools 37.0.0 and NDK
30.0.16248370. From the containing repository run
`python3 scripts/build-play-integrity.py`. The WebUI source link and packaged
SOURCE.txt identify the exact build commit. The repository contains the component
source, lockfiles and build scripts that retrieve the pinned native dependencies.
The root Zygisk libraries compose PI and Device Faker; classes.dex, scripts and
the arm64 CLI are included in the single installable Duck ToolBox module.
The host CLI and tests build with `cargo test --manifest-path Cargo.toml` here.

Root managers load `zygisk/arm64-v8a.so` and `zygisk/armeabi-v7a.so`. By default
the payload directory is `/data/adb/modules/duck-toolbox/play-integrity`; the data
directory is `/data/adb/play-integrity`. Native builds may override PIF_MODULE_DIR
and PIF_DATA_DIR for build/test fixtures. The CLI accepts PIF_ENGINE_MODULE,
PIF_ENGINE_DATA and PIF_ENGINE_SYSROOT; all must be absolute paths.

Call `pif-engine stage setup`, `stage early`, `stage late` and `stage uninstall`
from the corresponding root-manager lifecycle. The late stage must run after
boot completion. No network request occurs at boot. Enabling is the default;
an absent or empty configuration keeps full-mode injection and global compatibility
scripts idle. Explicitly saving `mode=scripts-only` permits compatibility scripts
without a fingerprint. Without a configuration, the original property handler
retains ownership; explicit scripts-only mode can take ownership without a
fingerprint. The independent boot-hash setting retains its existing behavior.

## Public process interface (version 1)

`pif-engine request OPERATION` reads optional JSON on stdin and prints one object:
`{ "ok": true, "api": 1, "data": ... }`, or an error with code/message/details.
Exit status is nonzero on failure. The protocol does not link either executable
to the other and is usable from an ordinary root shell.

| Operation | Input / result |
| --- | --- |
| power / disable | Minimal power state / independent fail-closed stop; cancels generation, restores recorded compatibility changes, reports incomplete cleanup |
| status | Options, prerequisites, conflicts, boot evidence and reboot requirement |
| config | Canonical prop/JSON content, entries and revision |
| preview | content, format (auto/prop/json), optional migrate/advanced/force; returns changes and base revision |
| prepare | Preview content plus revision; durably saves a backup token before publication |
| apply | Preview content plus revision and optional prepared backup token; atomic publication and backup token |
| restore | backup token; refuses to overwrite a newer configuration |
| options | enabled, mode (full/scripts-only), skipdelprop, skippersistprop, skipmatchdevice, app_replace_list |
| task-start | strong (default true), match_device, optional timeout_seconds (1–120, default 120); starts fingerprint generation |
| task-status / task-cancel | id; returns state, bounded output and candidate preview |
| restart | Restarts only DroidGuard and Play Store; reports each process result |
| diagnose | Status and bounded relevant logcat output; no claimed integrity verdict |

Fingerprint tasks run in private directories, last at most 120 seconds and never
install a candidate or edit another module's security patch policy. Completed tasks
retain their result and a bounded log; downloaded working files are removed. HTTPS uses
certificate validation and a bounded response body. A task's estimated expiry
is upstream's heuristic, not proof of continued server acceptance.

An enabled conflicting PIF/USNF module prevents this engine from injecting or
running its property scripts. The engine does not uninstall other modules.
ROM replacement paths require reboot. Only a single application/overlay entry under
`/{system,product,system_ext,vendor,odm,oem}/{app,priv-app,overlay}/NAME` is accepted.
Wildcards are permitted only in application names; overlay wildcards and the
`overlay/config` directory are rejected. The CLI and boot scripts both validate
the list, including expanded names, before creating any overlay artifacts. Existing prop-handler and boot-hash
settings are respected by lifecycle integration. Changes to global properties
cannot be undone merely by unloading code from an already running process.

No build or local test proves Play Integrity acceptance; hardware, ROM, Zygisk,
Google Play Services and current server policy require real-device verification.

Configuration previews include non-mutating consistency warnings. New configurations
default Provider compatibility off; explicit legacy values remain intact. Ordinary
migration leaves missing launch/board API levels unset. Canary keeps upstream's
documented heuristics and labels them as estimates. See [RULES.md](RULES.md) for
the pinned Magisk, KernelSU and AOSP sources and the complete behavioral limits.

`pif-engine migrate-file INPUT OUTPUT --advanced --format json` provides standalone
safe migration, preserving unknown fields and backing up an existing output as
a `.bak` sibling. The shell wrapper delegates to this command.

`pif-engine request disable` remains usable with invalid configuration and while
another write holds the engine lock. A private stop token closes future companion
transfers and script mutations first. Explicit enabling acknowledges the current
token; a stale enable cannot acknowledge a later stop. Configuration is preserved,
and generation/import/application never enable the engine implicitly. Use
`request power` to inspect the gate and any retained cleanup warnings. `options`
with `enabled=false` also takes this shutdown path before validating other fields.

DenyList changes are journaled per package/process before each mutation and read
back. Shutdown restores recorded values only while the relevant package still
matches the expected state. External edits and unrecorded historical values are
preserved. Magisk may terminate processes when entries are added under enforcement.
Permissive SELinux visibility permissions are also recorded and conditionally
restored; the enforcing state is never changed. Persistent compatibility properties
retain upstream restoration by writing the recorded original values, which may
overwrite later edits to the same properties; the power control discloses this. Active process hooks require a
PI process restart, and mounts/global properties require a device reboot. Other
toolbox features remain independent. Boot shell stages are bounded to 30 seconds;
explicit cleanup has a five-second bound and retains recovery files on failure.

Configuration library and observations use the same version 1 request protocol:
`profile-list`, `profile-save` (`name`, `content`), `profile-preview` (`id`) and
`profile-delete` (`id`). At most 12 immutable snapshots are retained. Previewing
uses the current applied revision; saving, previewing or deleting a snapshot never
applies configuration or enables the engine.
Unreadable snapshots are reported individually, remain counted toward the storage
limit, and require explicit removal; they do not hide healthy snapshots.

`record-list`, `record-save` and `record-clear` manage the latest 24 manually
reported observations. A record supplies `app`, optional `version` and `note`,
`basic`/`device`/`strong` (`unknown`, `pass`, `fail`) and the saved configuration's
`revision`. Changed revisions are rejected. Records capture configuration identity
and engine mode at recording time and are explicitly marked unverified.

`inspect` compares saved Build/property overrides with current system property
readings using a bounded read-only command. Existing modules may already affect
those system readings. Target-process values remain unobserved unless measured;
the comparison does not establish injection or a Google Play Integrity verdict.

## Pixel release candidates and activation

`pixel-devices` returns the supported device catalog with `bluejay` (Pixel 6a) as
its default. It includes 39 Pixel phone, foldable and tablet identities through
the Pixel 11 family; the WebUI searches model names and codenames. Launch API
levels and sources are documented in [RULES.md](RULES.md). A listed identity does
not guarantee that Google currently serves compatible release metadata.
`pixel-builds` accepts an optional `device` and retrieves release
metadata from Google's Android Flash Tool over certificate-verified HTTPS. It
uses the public web client configuration at request time, allows no redirects,
limits each request to five seconds and each response to 2 MiB, and retains at
most 40 builds per device. The private catalog expires after 24 hours. This web
metadata interface can change independently of this engine; unsupported responses
fail explicitly without modifying the applied configuration.

`pixel-preview` accepts `device`, a fetched `build` identifier, and its
`incremental` version. Legacy requests without `incremental` require a unique
build identifier. Selection changes and refetches invalidate previous previews.
It constructs a
release fingerprint and paired Build/property fields from that build, preserves
existing explicit advanced options, and does not apply configuration or enable
PIF. The metadata does not supply an exact security patch level, so the candidate
leaves patch fields unset instead of inferring them from the build ID or release
date. The first shipped API is device metadata, separate from the running OS SDK.

Previews can include `provenance` (source, device, build, fetch time and whether an
exact patch date is known). New official Pixel records also carry
`identity_bound=true`, the complete fingerprint, incremental version, model and
launch API. Snapshot save and read validate these against the identity fields and
any paired properties. Older records without this binding remain readable and
are identified as limited source records in the WebUI. `profile-save` can retain
this optional metadata with an immutable candidate snapshot. Provenance is descriptive and not a verified
integrity verdict. Official release metadata: <https://flash.android.com/>.
Google also distinguishes release dates from patch dates at
<https://developers.google.com/android/images>.

The separate **Pixel property library** tab uses the community archive at
<https://github.com/VictorGabriel7Dev/build.prop>. `pixel-archive-catalog` resolves
the repository's main commit from its public Atom feed, validates `index.json`,
and stores a private catalog for up to 24 hours. `pixel-archive-preview` accepts
`source_revision`, `device`,
`channel` (`release`, `beta`, `canary`), `build` and `incremental` from that catalog.
It downloads the selected property file at the same commit, checks its product
identity, build, patch date, launch API and vbmeta digest against the index, and
imports only identity fields and available system/vendor/boot patch dates into a candidate.
The product partition's fingerprint is used even when the system partition has
a generic fingerprint; Canary product suffixes and release codenames are preserved.
Explicit advanced options remain intact. Missing channels are unavailable.

Each HTTPS request validates certificates, disallows redirects, times out after
five seconds and caps the commit feed at 256 KiB and index/property bodies
at 64 KiB. Errors leave the applied configuration unchanged. No archive scripts
are downloaded or executed. There is no boot-time fetch. The existing Google
metadata source remains independently available.

Archive provenance retains the commit, channel, build, incremental version,
patch date, property-file SHA-256 and vbmeta digest with saved snapshots. These
values describe the source, not an authenticated integrity verdict. The boot hash
is shown for reference and never applied by this import. Previewing or saving a
candidate does not enable PIF; applying and optional keystore patch synchronization
use the existing explicit preview/transaction flow.

Configuration application records a durable process restart requirement before
publication. Keystore patch transactions conservatively record a device reboot
requirement. Process restarts cannot clear a device reboot requirement; a changed
known boot identity clears requirements from the previous boot. Missing boot
identity does not prove a reboot. Status includes `process_restart_required`.

`inspect` additionally reads bounded `PIF/Observation` log records. Native injection
emits a single snapshot after specialization: actual Build values, existing property
callback readings, hook installation result, configuration SHA-256, PID/start time,
boot identity and observation time. Only observations matching the saved revision,
current boot and a currently live target process are returned. A snapshot can be
unavailable because of log rotation, permissions or a process that has not restarted.
It does not cover subsequent mutations, every property API or server acceptance.

`record-import` accepts a decoded Play Integrity report and the current configuration
`revision`. It stores only an allowlisted summary, never the token, nonce or account
identifiers. Present labels are reported as passes; missing labels are `omitted`
with an unknown cause, not failures. Imported reports remain user-supplied and
unverified. Test context includes available system SDK, GMS/Play Store version codes,
boot identity, activation requirements and optional `installation_source`.
