# Embedded Device Faker

`engine` contains the adapted GPL-3.0-or-later source of Seyud/device_faker,
pinned by `UPSTREAM_REVISION` and `UPSTREAM.json`. Original LICENSE and NOTICES
are retained. This is a normal source component; no Git submodule initialization
or additional root-manager module is needed. The manager and Java density helper
use the same license. The MIT `duckd` adapter uses a bounded process interface.

## Dependency licensing

Redistribution permission remains unconfirmed for the linked `prop-rs` and
`prop-rs-android` dependencies from KernelSU2/ksu_props at
`ddb6ee7294467f7f25bad2118e9e24eee104144b`. That revision contains no license
text or package license declaration; its upstream 5ec1cff/resetprop-rs also
provides no identified license. The Device Faker GPL grant does not establish
permission for these separately authored dependencies. Obtain an applicable
license from their rights holders or replace them with an implementation whose
redistribution terms are established before distributing the linked binaries.
The engine build records dependencies without a declared license or nonempty
license file. Module packaging rejects such a build, including older build
metadata without this check. Passing this check only establishes the presence
of a declaration; it does not establish license compatibility or permission
for code beyond that declaration's scope.

## Build and verification

```sh
cargo test --locked --manifest-path components/device-faker/manager/Cargo.toml --target-dir target/device-faker-manager
python3 scripts/test-device-faker.py
python3 scripts/test-zygisk-runtime.py
python3 scripts/build-play-integrity.py
```

The combined build invokes `build-device-faker.py`, then links the ARM64 static
engine into the toolbox’s one Zygisk library. ARM32 retains PI support only.
Requirements: Rust 1.89+, Android Rust target, Python 3.12+, JDK 21, Android SDK /
Build Tools 37 and NDK 30.0.16248370. Lockfiles and pinned dependency revisions are
part of the corresponding-source archive included in the installation ZIP.

## Lifecycle and compatibility

The API-v4 dispatcher runs PI first. If PI retains the library for a selected
process, Device Faker does not modify that process. Otherwise Device Faker runs
its own callbacks. Library unload occurs after both engines finish. Companion
connections carry a bounded, versioned routing frame; each engine owns its state.

Startup serializes legacy property updates and PI late initialization before
publishing a boot-specific Device Faker readiness token. Duplicate successful boot
passes skip global mutation. Failure keeps injection
closed. Enable acknowledges the current stop token; sessions retain this generation, and a stale request cannot cancel
a later stop. Parent disable/removal, pending cleanup, absent readiness or a loaded
standalone `device_faker` closes the gate. A standalone disable flag must predate
the current boot. Reboot after installation or disabling a conflicting module.

Private state lives under `/data/adb/duck-device-faker`. Standalone configuration
is read only for explicit import preview. Empty defaults select no apps. Saving,
importing and restoring do not enable injection. Stop remains available while
configuration is invalid or another operation is running. Existing process-local
Build/property values require restarting the app to disappear.

Property clones own their mappings and remain alive for the process lifetime.
Unknown bionic layouts skip property-memory changes. Real bionic mappings are not
unmapped, replaced with MAP_FIXED or owned by a foreign Rust mapping object.
`hide_maps` is ignored with a warning. Build-field failures clear their JNI
exceptions instead of throwing into application startup. Missing Build fields are skipped individually; other JNI failures stop that
process’s hook, and preexisting exceptions remain pending.

Global resetprop and display-density options require acknowledgement. Sessions
bind package, Android user and PID start time; foreground identity is confirmed
through the window manager. Cleanup restores only values still owned by the
session, preserving later external edits. The density helper uses explicit user
IDs and a private durable recovery journal. Failed recovery blocks enabling and
retains recovery data. Foreground global-property sessions publish an ownership marker first. Companion
loss or PID reuse blocks new injection until reboot; original global-property
backups remain in companion memory. Foreground queries have one monitor; session mutations remain serialized, and
Binder callbacks only notify the monitor. Display 0 focus authorizes changes;
conflicting unscoped focus cannot activate a global profile. The watchdog interval is 500 ms, with bounded platform
commands; it is not a cleanup deadline. Reboot if global cleanup remains pending.

## Process contract, version 1

`device-faker-engine request OPERATION` reads an optional JSON object and returns
`{ "ok": true, "api": 1, "data": ... }` or a structured versioned error. Failures
exit nonzero. Requests are limited to 128 KiB, TOML to 48 KiB; the adapter bounds
runtime to 15 seconds and output to 2 MiB. `DEVICE_FAKER_SYSROOT` selects an absolute
host fixture root. It is not a native injection override.

| Operation | Behavior |
| --- | --- |
| status / config | Power, readiness, conflicts, cleanup state and configuration snapshot |
| preview / import-preview | Validated summary, warnings and base revision; no writes |
| apply | Revision-checked configuration publication and one-step backup; optional global acknowledgement |
| restore-preview | Preview the prior configuration before applying it |
| enable | Requires valid targets, current revision and stop token, readiness and no conflict |
| disable | Publishes a stop token without waiting for configuration parsing or its lock |

`stage boot` owns the ordered property/PI pass and readiness publication;
`stage setup` creates empty defaults; `stage ready` recovers density and publishes
readiness; `stage blocked` records startup failure; `stage uninstall` attempts
cleanup and preserves unresolved recovery data.

Status always reports `injection_verified: false`. Host tests and cross-compilation
do not prove Zygisk loading, vendor bionic layout compatibility, runtime field values
or effects on particular apps. See [technical references and validation](../../docs/device-faker.md).
