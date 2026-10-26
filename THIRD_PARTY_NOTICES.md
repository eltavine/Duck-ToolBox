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

Binary distributions include `licenses/play-integrity`, dependency notices,
build metadata and a `SOURCE.txt` link pinned to the exact build commit. The
feature footer links directly to `components/play-integrity` at that commit.
The repository also contains the build script that fetches pinned dependencies
and applies the recorded ShadowHook change. No duplicate source archive is
included in the installable module or uploaded as a separate release asset.
The matching Git source and build materials must remain publicly accessible
for as long as object code is distributed.

Existing third-party notices, including `module/licenses/rkp-v1-oracle.txt`, remain
applicable to their respective components.

Kernel module ELF inspection links the `object` crate (version locked in `Cargo.lock`)
with only its ELF reader enabled in production. It is used under its MIT license;
the copyright and license are included in `module/licenses/object-MIT.txt`.

TrustKernel provisioning loads the device's existing `libkphproxy.so` at runtime in
an isolated worker. No KPH vendor executable or library is distributed by this module.
The root MIT license covers the independently written adapter, not the device's vendor
libraries. Vendor ABI and firmware compatibility still require device validation.
