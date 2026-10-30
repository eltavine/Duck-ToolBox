# Licensing boundaries

Duck ToolBox's independently written management code remains under the root MIT
license. The Play Integrity engine is a separate program distributed in the same
installation archive. Its native library is loaded by Zygisk into selected Android
processes; it is not linked into the `duckd` management executable. Management uses
the documented engine CLI and configuration formats.

All code derived from Play Integrity Fork, including translated or adapted code,
is kept in the independently built `components/play-integrity` project under GPLv3,
except dependencies with their own notices. See its LICENSE, NOTICE and UPSTREAM.json.
The root MIT license does not replace those terms.

Binary distributions include both engine licenses, dependency notices, build
metadata and source revision identifiers. `webroot/licenses/corresponding-source.tar.gz`
contains the exact committed source plus Cargo-vendored dependencies and pinned
native dependencies. Keep that archive with redistributed binaries, including local
builds whose Git commit has not been published. The feature footers download this
archive. Build scripts and lockfiles are included.

The composite Zygisk library statically links the Play Integrity and Device Faker
engines and the callback dispatcher. This combined library is distributed under
GPL-3.0-only; original dependency licenses and copyright notices still apply.
The `duckd` executable communicates with the separate management processes and does
not link either GPL engine. The root MIT license does not replace component terms.

Existing third-party notices, including `module/licenses/rkp-v1-oracle.txt`, remain
applicable to their respective components.

Kernel module ELF inspection links the `object` crate (version locked in `Cargo.lock`)
with only its ELF reader enabled in production. It is used under its MIT license;
the copyright and license are included in `module/licenses/object-MIT.txt`.

TrustKernel provisioning loads the device's existing `libkphproxy.so` at runtime in
an isolated worker. No KPH vendor executable or library is distributed by this module.
The root MIT license covers the independently written adapter, not the device's vendor
libraries. Vendor ABI and firmware compatibility still require device validation.

Linux environment import uses `tar` 0.4.46 and `filetime` 0.2.29 under their MIT
licenses, included in `module/licenses/tar-MIT.txt` and `filetime-MIT.txt`.
Gzip decoding uses the existing locked `flate2` / `zlib-rs` backend.
The separate static ARM64 Tini executable is pinned to upstream v0.19.0 and its
SHA-256 in `components/linux-env/tini.json`; its original copyright and MIT license
are included in `module/licenses/tini-MIT.txt`. It is copied into each environment's
private runtime mount and is not linked into `duckd`.

Device Faker source is embedded under `components/device-faker/engine`, derived
from Seyud/device_faker at the revision in `UPSTREAM_REVISION`. It is a normal
source directory, without a Git submodule or separate root-manager module. The
original GPL license, NOTICES and copyrights are retained. Adaptations, the
management CLI and density helper use GPL-3.0-or-later. The composite
dispatcher uses GPL-3.0-only, matching the linked PI engine.
See [the component documentation](components/device-faker/README.md).

Redistribution permission for Device Faker's linked `prop-rs` and `prop-rs-android`
dependencies is unresolved. Their pinned KernelSU2/ksu_props revision
`ddb6ee7294467f7f25bad2118e9e24eee104144b` has no license text or package license
declaration. The component GPL notices do not supply that missing permission.
Resolve the dependency terms or replace those dependencies before distributing
the combined library. See the component documentation for the recorded scope.
Module packaging rejects engine builds with missing dependency license
declarations, and rejects older build metadata that lacks this check.
