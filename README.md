# Duck ToolBox

Duck ToolBox is a root module (KernelSU, APatch or Magisk) that puts Android attestation
and system tools behind one WebUI. The backend is a Cargo workspace of single-purpose crates
compiled into one JSON CLI (`duckd`); the WebUI is a set of feature folders that the app
discovers at build time. Features only meet through versioned contracts, so each one can be
changed, removed or added on its own.

## Tools

- **Play Integrity**: an independently built GPLv3 engine derived from Play Integrity Fork,
  with configuration previews, migration, fingerprint generation, compatibility settings and
  diagnostics in the WebUI. It is enabled by default; injection requires Zygisk and a saved
  nonempty configuration. Full-mode global scripts also wait for configuration; explicitly
  selected scripts-only mode can run without a fingerprint. A separate stop control works
  during tasks, invalid configuration and pending recovery; configuration is retained. The management code remains MIT; see [licensing boundaries](THIRD_PARTY_NOTICES.md).

- **Device Faker**: per-app device profiles and reusable templates from
  [Seyud/device_faker](https://github.com/Seyud/device_faker), embedded as a normal source component.
  The WebUI edits TOML, builds template/app drafts, previews file or standalone-config
  imports, reports conflicting assignments and restores the previous configuration.
  Configuration and power controls are independent; empty defaults target no apps.
  Its GPLv3 engine shares the toolbox’s single Zygisk library with PI,
  with explicit callback ordering and parent disable/removal gates. Reboot after installation.
  Requires Zygisk and ARM64 apps. Global property/DPI options require acknowledgement;
  process-local changes require an app restart, and failed global cleanup needs reboot.
  Online template downloads are not embedded. Actual injection needs device validation.
  See [build, lifecycle and protocol](components/device-faker/README.md).

- **Tricky Store manager**: a complete port of KOWX712's
  [Tricky Addon: Update Target List](https://github.com/KOWX712/Tricky-Addon-Update-Target-List),
  for whichever keystore module is active:
  - **Tricky Store**: `config.ini` with per-app modes (`pkg`, `pkg!`, `pkg?`) and per-app
    policy, and the legacy `target.txt` + `security_patch.txt` layout.
  - **TEESimulator**: `config.json` profiles (mode, patch levels, identity, auto-include).
  - **OhMyKeymint**: `injector.toml` scoop and every `config.toml` `[trust]` key.

  It has the same features as upstream:
  - Target list with search, select all, deselect all and refresh.
  - Mode sheet on long press, and per-app policy.
  - Actions: select from DenyList (Magisk), deselect unnecessary apps (online list plus
    Xposed modules), add system apps, auto-add new apps.
  - Keyboxes from AOSP, generated "unknown", a local file, the keybox repository (the same
    iframe protocol) or custom providers (add, edit, remove, reset, import, export).
  - Prop handler and Verified Boot hash, schema-driven default policy with *Today*, help.
  - Keyboard shortcuts (Ctrl+A/D/F/S, Esc), the back gesture closing dialogs, and upstream's
    23 translations, including right-to-left languages.
  - Optionally, a WebUI entry on the keystore module itself, and the TSupport-Advance
    auto-target stop flag.
- **RKP Workbench**: Remote Key Provisioning.
  - **Import DICE materials** accepts an existing CBOR DICE chain and explicitly typed
    signing material. No existing CSR, Qualcomm interface or device collection is required.
  - Verifies the chain and leaf-key match locally, constructs a fresh CSR v3 with the
    imported chain, obtains a new server challenge, and exports a keybox only after the
    returned certificates match the newly generated private key and pass local checks.
  - Offline validation and diagnostic CSR construction are separate from online issuance.
    Network failures do not trigger a local fallback or an automatic signing retry.
  - The older seed/profile workflow remains an explicitly labelled **experimental
    degenerate-chain mode**. Its saved seed semantics are unchanged. Import mode requires
    explicit device and issuance configuration and never falls back to experimental mode.
  - Private output uses owner-only permissions, atomic publication and readback checks;
    installation through the Tricky Store manager remains a separate action.
  - See [input formats, supported combinations and verification limits](docs/rkp-materials.md).
    Synthetic server tests cover the local flow; real-material server acceptance and a
    successful production keybox export have **not** been verified for the import path.
- **Device RKP Issuer**: the same keybox export, but signed by the device itself instead of
  a seed you hold.
  - Asks this device's Qualcomm KeyMint TA for a `CSR_V2` over a P-256 key we generated,
    submits the result to Google's RKP server and exports `keybox.xml` with the private key.
  - Supports a legacy V1 AIDL path for Qualcomm PureSoft and MediaTek firmware with a zero HBK.
    The public-key MAC and CSR are constructed in Rust; a small isolated bridge calls
    the device's own AIDL proxy, using the EEK curve reported by its HAL.
    This method is adapted from [LazyBonesLZY's rkp-v1-oracle](https://github.com/LazyBonesLZY/rkp-v1-oracle)
    at revision `b28480c98c1256753aed140c6ff86d0336f55c45` (MIT; see `module/licenses/rkp-v1-oracle.txt`).
    MediaTek uses only this AIDL adapter. Native V1 and V2 require Qualcomm identification
    as well as the vendor library and device node. Device ID provisioning uses a separate
    TrustKernel KPH adapter described below. Real-device
    compatibility and production server acceptance for MediaTek remain unverified.
  - Select `auto`, `v1`, `v2` or `v1-native` in the WebUI or with `rkp-device provision --protocol`.
    Automatic mode tries the available methods in order: V2, AIDL V1, then native V1.
    Each failed signing, CSR validation, server request or certificate validation moves
    to the next method with a fresh challenge. It stops at the first complete success;
    if every method fails, the error retains each method's name, code and reason.
    An explicit selection tries only that method. Files are written only after issuance
    and certificate validation succeed; a local save failure does not repeat issuance.
  - Native V1 uses legacy vendor command `0x311e`, adapted from
    [aviraxp's native V1 contribution](https://github.com/eltavine/Duck-ToolBox/pull/19).
    It supplies a fresh MAC key, the public-key MAC and the complete selected EEK chain
    directly to the TA, reusing the isolated vendor worker. It needs the same library
    and device node as V2, and does not require Java or a system RKP APK at runtime.
    The vendor framing and EEK curve compatibility require real-device validation.
    It shares V1 CSR assembly and structural validation with the AIDL adapter,
    preserving opaque DeviceInfo and ProtectedData.
  - Checks Android arm64, root and Qualcomm SoC before sending anything. V2 needs
    `/dev/smcinvoke` and `libkeymasterdeviceutils.so`; AIDL V1 needs `app_process` and a
    system RKP APK containing the matching AIDL classes. Neither check proves firmware support.
  - V1 encrypts the DICE and signing evidence for the server. Its local report is
    explicitly structural only (`local_verify: null`); V2 retains local signature checks.
    Results include `csr_protocol` (`v1` / `v2`) and `csr_backend` (`aidl` / `native`).
    The `attempts` field records method failures and the final successful method; the
    WebUI displays the same history.
  - Starts its isolated worker with vendor library search paths while retaining the
    default namespace's system dependency links. Library loading and the firmware ABI
    still need to work on the device; prerequisite checks alone cannot confirm them.
  - The exported keybox contains a software-generated private key. Output directories
    are private, and both the keybox and the private key are saved with mode `0600`.
  - Only works on some devices, and only RKP certificates can be issued. Both are stated in
    a dialog before the tool opens and again before a request is sent; the backend refuses
    to run without an explicit acknowledgement.
  - On a OnePlus PJX110 running Android 16, loading from `/data/adb` and calling the TA
    were verified. Both this implementation and the reference client received TA status
    `-18` for the software-key `CSR_V2` request; that test did not issue a keybox.
    The V1 adapter has host protocol/bridge tests and an Android build check; device
    compatibility and successful Google issuance still require a real-device run.
- **TEE Attestation Extractor**: offline extraction and verification of existing captures.
  - Extracts complete Keybox XML documents and PEM bundles, including PEM embedded in binary
    captures. Supports EC P-256/P-384 and RSA 2048–8192 private keys.
  - Matches each private key to a certificate and verifies every signature along one
    unambiguous leaf-to-root path. Reports certificate validity separately; root trust and
    revocation are not checked.
  - Compares raw or hex-encoded 32-byte identity material to the signing leaf of a verified
    RKP CSR. Distinguishes Ed25519 seeds, P-256 seeds (`mod(n - 1) + 1`) and P-256 scalars.
  - Verification returns summaries only. Explicit exports save material and reports in a
    private directory (`0700`) with files restricted to the owner (`0600`). An identity
    mismatch prevents export. Exported keyboxes can be installed through the manager.
  - Processes device files up to 256 KiB locally; inline uploads are limited to 32 KiB per
    file so two base64 inputs fit the root manager's shell transport. Device acquisition
    and firmware patching are outside this feature's current capabilities; a local match
    does not prove server acceptance.
- **Device ID Provisioner**: retains Qualcomm Keymaster provisioning through
  `libQSEEComAPI.so` and supports experimental MediaTek TrustKernel provisioning through
  the device's `libkphproxy.so`. Both paths auto-fill identifiers and support dry runs.
  TrustKernel prepares and writes `rkp_flag` and `attestation_ids`, with private request
  files, isolated vendor calls, timeouts and partial-write reports. Its existence check does not
  verify payload equality; MEID entries are explicitly reported as omitted on this path.
  See [backend contracts, ABI evidence and compatibility limits](docs/trustkernel-provisioning.md).
- **Kernel modules**: Linux kernel module inventory and runtime management.
  - Reads loaded modules from `/proc/modules` and separately labels entries visible only
    in `/sys/module`; those may be built into the kernel and are never offered for unloading.
  - Shows memory size, reference counts, dependents, version, taint and readable parameters.
    Parameters are read-only; unreadable state is reported separately from empty results.
  - Scans standard Android module directories and inspects an explicitly chosen uncompressed
    `.ko` file using the ELF `.modinfo` section, preserving repeated metadata entries.
  - Manual loading requires confirmation and the SHA-256 from a prior inspection. The worker
    verifies the file again and submits those exact bytes to the kernel, without force flags.
    Dependencies must already be loaded. Vermagic release comparison is advisory because
    symbol CRCs can permit a different release prefix; kernel ABI, symbol, signature and
    policy checks remain authoritative. Loading requires a little-endian ARM64 ELF64 image.
  - Unloading is limited to live modules reporting zero references and no dependents or holders.
    The kernel makes the final decision. No forced unloading or startup-rule edits are exposed.
    The backend blocks unloading `kernelsu` by name and rejects unreadable holder state.
    This protection list does not identify every root fork or critical device driver.
  - Mutations are serialized in an isolated worker and require Android ARM64 root outside a test sysroot.
    A 30-second worker timeout reports an unknown outcome: terminating userspace cannot undo
    or reliably cancel a kernel init/exit routine. Missing or abnormal worker results also
    report an unknown outcome. Refresh before further action.
  - See [source contracts, interface guidance and verification limits](docs/kernel-modules.md).
- **Linux environments**: import trusted ARM64 `.tar` / `.tar.gz` root filesystems,
  start and stop them, copy a terminal entry command, configure foreground services,
  read bounded logs and opt into boot startup. Private mounts use PID namespaces when
  available, with cgroup v2 cleanup on kernels without them. Commands run as ROOT and
  share Android's kernel and network; chroot is not a security sandbox. See
  [requirements, lifecycle and verification](docs/linux-env.md).
- **ZRAM & swap**: existing ZRAM swap devices, compression and physical-memory statistics,
  capacity/algorithm/priority changes, same-boot parameter restoration and opt-in boot plans.
  Runtime changes require confirmation and Android ARM64 root. Devices managed by mmd,
  using writeback/secondary compression, mounted or held by another block device are protected.
  Other swap areas are read-only. See [source contracts and verification limits](docs/zram.md).
- **Module & updates**: version and device summary, stable and canary updates with
  changelog, output files, command history, uninstall and reboot.

Every tool is translated into 16 languages: English, Simplified and Traditional Chinese,
Japanese, Korean, Spanish, French, German, Italian, Brazilian Portuguese, Russian, Arabic,
Vietnamese, Indonesian, Ukrainian and Polish. Tricky Addon's other translations cover the strings the Tricky
Store manager shares with it.

## Architecture

```txt
crates/
  duck-core/          Shared runtime: paths, JSON envelope, Feature trait, command log,
                      and a Sysroot so every feature is testable off-device.
  duck-platform/      Device adapters: props/resetprop, pm, modules, root managers, HTTP.
  duck-rkp/           RKP workbench.            duck-rkp-device/  Device RKP issuer.
  duck-tee-attestation/ Offline capture extraction and identity verification.
  duck-device-ids/    Device ID provisioning.   duck-tricky-store/ Keystore module manager.
  duck-system/        Module lifecycle, updates.
  duck-kernel-modules/ Linux kernel inventory, ELF inspection and isolated runtime operations.
  duck-zram/          ZRAM inventory, swap configuration and opt-in boot profiles
  duckd/              The binary: builds its CLI from the compiled-in features.
xtask/                Repository checks (`cargo xtask line-limit`).
ui/src/
  app/                Shell: router, app bar, home and settings; discovers features.
  core/               Bridge to the root manager, validated duckd client, i18n, theme,
                      extension points and shared components.
  features/<id>/      One folder per tool: index.ts, api.ts (zod), locales/, pages, mock.ts.
  components/ui/      shadcn-vue components, restyled for Material 3 Expressive on phones.
module/               Module payload: scripts, then the built WebUI and backend binary.
```

What keeps it modular and resistant to breaking changes:

- **Features never import each other.** Backend feature crates depend only on `duck-core`
  and `duck-platform`, except the Device RKP Issuer and TEE Attestation Extractor, which reuse
  the RKP workbench's public protocol primitives and verifier. WebUI features depend only on `core/`;
  ESLint (`eslint-plugin-boundaries`) rejects anything else, and only `core/bridge` may touch
  the KernelSU JavaScript API.
- **Extension points instead of imports.** A feature contributes to others through
  `contributes`: `keyboxTargets` (RKP installs its keybox through the Tricky Store manager),
  `settingsSections` and `homeWidgets`.
- **Versioned contracts.** Every command prints `{ ok, api, command, data, error, ts }`.
  `duckd features` lists the compiled-in features and their contract versions; the WebUI
  disables a tool whose backend is missing or on another contract. It validates every
  response with zod, so a changed shape fails loudly as `contract_mismatch`.
- **Keystore modules are adapters.** Each backend implements `ConfigAdapter` and describes
  its editable policy as a schema; the WebUI renders whatever the schema declares.
- **Mature tools, no home-grown frameworks.**
  - Backend: clap, serde, tokio, reqwest, the RustCrypto crates, rcgen and `toml_edit`
    (which keeps comments).
  - WebUI: Vue 3, vue-router, Pinia, TanStack Query, vue-i18n, zod, Tailwind CSS 4 and
    shadcn-vue on reka-ui, with VueUse.
- **No source file over 600 lines.** `cargo xtask line-limit` fails CI for any source file
  in any language, including scripts without an extension; ESLint's `max-lines` flags it
  while editing.
- **Dependency updates are reviewable.** Dependabot groups minor and patch updates; every
  major update gets its own pull request and a full CI run.

### Adding a tool

1. **Backend:** create `crates/duck-<name>` with a `clap` subcommand enum and a
   `pub static FEATURE: ClapFeature<Command>` (see `duck-system`). Add it to `duckd` as an
   optional dependency and cargo feature, and one entry in the `FEATURES` array in
   `crates/duckd/src/main.rs`.
2. **WebUI:** create `ui/src/features/<id>/` with an `index.ts` exporting
   `defineFeature({ id, namespace, icon, order, routes, messages })`, an `api.ts` that calls
   `duckd()` with zod schemas, a `locales/<language>.json` for each of the 16 languages
   (`src/locales.test.ts` fails on a missing key), and optionally a `mock.ts`. The home page,
   router, i18n, availability check and dev mock pick it up automatically.

## Module lifecycle

| Stage | What runs |
| --- | --- |
| `customize.sh` | Checks the root manager, ABI and API level; prepares `/data/adb/duck-toolbox`; normalizes the boot hash; drops `action.sh` where the manager has a WebUI button; requests hot install from metamodules that support it. |
| `post-fs-data.sh` / `late-load.sh` | Early pass of the sensitive-prop handler (`resetprop -n`, never `setprop`). KernelSU's late-load mode runs `late-load.sh` instead of `post-fs-data.sh`. |
| `service.sh` | Repairs the runtime directory. On Magisk, which has no boot-completed stage, it waits for boot and runs `boot-completed.sh`. |
| `boot-completed.sh` | Late prop pass, keystore-module entry links, auto-add of new apps, the module description (KernelSU temporary `override.description`), and the TSupport-Advance flag. |
| `action.sh` (Magisk) | Opens the WebUI in KSUWebUIStandalone or WebUI X, installing KSUWebUIStandalone if neither is present. |
| `uninstall.sh` | Removes entry links, flags, the boot hash and `/data/adb/duck-toolbox`. |

Saved data lives in `/data/adb/duck-toolbox/var`, outside the module directory, so updates
keep it.

**Updates.** The stable channel follows `updateJson` in `module.prop`. The canary channel
reads the newest CI artifact through nightly.link: CI uploads the module directory as
`duck-toolbox-<version>-<commit count>-canary`, and the commit count is its `versionCode`.
Each successful package on a `main` push or manual CI run also publishes the module ZIP
as a GitHub prerelease. Tags use `nightly-<8-character commit hash>-<UTC timestamp>`
(for example, `nightly-1234abcd-20261011T120000123456Z`). These releases do not replace
the latest stable release; Telegram uploads continue on push builds.
The **Stable** workflow runs when the `version` field changes in
`module/module.prop` or through a manual request. Metadata-only edits skip publication.
It reuses the CI checks and builds, preserves `version`
and `versionCode` from `module/module.prop`, publishes `v<version>` as the latest
non-prerelease release, and sends the same ZIP to Telegram with a `stable` label.
Before running it on `main`, increase `versionCode` beyond all accepted releases and
update `update.json` to match the version and `duck-toolbox-<version>.zip` asset.

**KernelSU Modules repository.** The root `module.yaml` supplies the module listing
metadata and selects `duck-toolbox-*.zip` release assets. Published ZIPs contain
`module.prop` at their root; the current builds target Android API 26+ on arm64-v8a.
The installer rejects other architectures before preparing runtime data.
The repository must enable release immutability and be registered through the
[KernelSU Modules developer portal](https://developers.kernelsu.org/).
Nightly prereleases are checked and recorded but are not displayed as stable versions.
Every new version must have a `versionCode` greater than all previously accepted
stable and prerelease versions; nightly CI uses the commit count. Rebuilding the same commit
creates another nightly release with the same `versionCode`, which is treated as the
same prerelease version. A stable release needs a higher `versionCode`, or an accepted
prerelease can be promoted by clearing its prerelease flag.

## JSON CLI

```txt
duckd features | describe
duckd rkp profile show|save|clear|detect
duckd rkp info | provision | keybox | verify <file>
duckd tee-attestation status
duckd tee-attestation keybox <capture> [--format auto|xml|pem] [--export]
duckd tee-attestation seed <capture> --csr <csr.cbor> [--format raw|hex] [--derivation ed25519-seed|p256-seed|p256-scalar] [--offset <decimal>] [--export]
duckd device-ids defaults | provision
duckd tricky-store status | save | props | auto-apply
duckd tricky-store keybox install <file> | import | set-aosp | generate | fetch --url <u> [--decode <steps>]
duckd tricky-store keybox providers list | save | reset | import <file> | import-content | export [--path <file>]
duckd tricky-store apps xposed | denylist | unnecessary [--refresh]
duckd tricky-store entry status | enable | disable | apply | remove
duckd system info | update check|install [--channel stable|canary] | artifacts | files --path <dir> --extension <ext>
duckd system log [--limit <n>] | open-url <url> | uninstall | reboot
duckd kernel-modules status | details <name> | files | inspect <device.ko>
duckd kernel-modules load --stdin-json
duckd kernel-modules unload --stdin-json
duckd zram status
duckd zram apply --stdin-json
duckd zram restore --stdin-json
duckd zram save --stdin-json
duckd zram forget <zramN>
```

Commands that take input read JSON from stdin with `--stdin-json`.

Kernel module loading accepts
`{"path":"/data/local/tmp/example.ko","sha256":"<digest from inspect>","parameters":"debug=1","confirmed":true}`.
Unloading accepts `{"name":"example","confirmed":true}`. These operations affect the
running kernel and can stop or restart a device even when preflight checks pass. The
WebUI asks for confirmation before submitting either operation.

`rkp-device status` identifies the SoC from manufacturer aliases, SoC part numbers,
hardware properties and the kernel's [SoC family/machine attributes](https://www.kernel.org/doc/rustdoc/latest/kernel/soc/struct.Attributes.html). It reports the raw
evidence and distinguishes Qualcomm, MediaTek, another vendor, unknown and conflicting information.
Unknown or conflicting information blocks issuance rather than guessing. These local
checks do not establish firmware ABI compatibility or server acceptance. The status format
uses feature contract 6; update the backend and WebUI together.

For offline identity checks, raw input must contain exactly 32 bytes unless `--offset`
selects a 32-byte slice of a larger capture. Hex input must contain exactly 64 hexadecimal
digits, with optional surrounding whitespace. The derivation is explicit: a P-256 scalar
cannot be substituted for a seed. The CSR must be a complete CBOR `AuthenticatedRequest`
with a valid DICE chain and KeyMint signature. Checks do not submit a request to a server
or change an RKP profile. The saved identity is `identity.bin`; its derivation and CSR digest
are recorded separately in `report.json`. Keep both files together.

The WebUI accepts device file paths (up to 256 KiB) or binary files from the system picker
(up to 32 KiB each). Both offline commands also accept `--stdin-json` with `source: {"path": "..."}` or
`source: {"base64": "..."}`, plus `format` and `export`. Identity requests additionally take
`csr` in the same file format, `derivation`, and an optional raw `offset`. CLI input options
and JSON requests are separate modes; put all operation options in the JSON request.

## Building and testing

Requirements:
- Rust stable with the `aarch64-linux-android` target (`rust-toolchain.toml` installs it).
- Android NDK r30.
- JDK 17 or newer and Android SDK Build Tools 36.0.0 for the embedded V1 bridge.
  Set `ANDROID_HOME` or `ANDROID_SDK_ROOT`; `ANDROID_BUILD_TOOLS_VERSION` can select
  another installed version. Set `JAVA_HOME` or put `java` and `javac` on `PATH`.
  Host-only Rust builds do not require the SDK or JDK.
- Node.js 24 and pnpm 12 (`corepack enable` picks the version from `ui/package.json`).
  Type checking runs the TypeScript 7 engine through typescript-native-bridge, because
  stock TypeScript 7 no longer has the API vue-tsc needs.

The Android build compiles the V1 bridge with `javac`, converts it to `classes.dex`
with D8, and embeds the resulting DEX JAR in `duckd`. A V1 request writes the JAR
to a private temporary directory for `app_process` to load, then removes it after
the bridge exits. Devices do not need a JDK or SDK build tools.

```bash
cargo xtask line-limit
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python3 scripts/test_rkp_v1_bridge.py  # AIDL fixture, framing and error propagation (JDK)

# Device binary, e.g. with the NDK's clang as linker
cargo build --release --package duckd --target aarch64-linux-android

cd ui
pnpm install --frozen-lockfile
pnpm lint && pnpm format:check && pnpm test
pnpm build                 # type-check, then bundle into module/webroot
pnpm test:contract         # every API call through duckctl.sh and a host duckd build
pnpm test:e2e              # Playwright against a mock KernelSU host, with screenshots
pnpm dev                   # the same mock host in a desktop browser
pnpm locales:import <Tricky-Addon>/webui/public/locales/strings   # refresh translations
```

On Windows, `pwsh ./scripts/build.ps1 -PackageModule` builds everything with `cargo-ndk`
and writes the module zip to `dist/`.
The Windows helper pins the WebUI to the current Git commit automatically. For
manual WebUI builds set both `VITE_PIF_SOURCE_COMMIT` and
`VITE_DEVICE_FAKER_SOURCE_COMMIT` to `git rev-parse HEAD` before
running `pnpm build`; the module packager refuses moving-branch source links.

The bundled Play Integrity engine additionally requires JDK 21, Android SDK / Build
Tools 37 and NDK 30.0.16248370. Build it independently and validate both release channels:

```sh
python3 scripts/build-play-integrity.py
cargo test --locked --manifest-path components/play-integrity/Cargo.toml
python3 scripts/test-play-integrity-native.py
python3 scripts/test-play-integrity-java.py
python3 scripts/test-play-integrity-scripts.py
python3 scripts/package-module.py --channel stable
python3 scripts/package-module.py --channel nightly
```

Channel metadata is written into each ZIP without changing `module/module.prop`.
Nightly packages use the repository's commit-count version code and `canary` suffix.
Both release channels retain the GPLv3 license and dependency notices. The engine
build records its exact Git commit, and packaging rejects a dirty or mismatched
source checkout or WebUI source link. The installable ZIP includes `webroot/licenses/corresponding-source.tar.gz`,
with the exact source commit, locked Rust dependencies and pinned native build
materials. Both feature footers offer this source download; Git URLs identify
the build revision even for a local commit that has not been pushed.
The pinned build/dependency script is `scripts/build-play-integrity.py`.
See [the rule review](components/play-integrity/RULES.md) for original sources
and required device validation.

The power control uses `duckd play-integrity disable` (standalone:
`pif-engine request disable`) and `play-integrity power` for status. Disable
publishes a private stop token before configuration parsing or locking, cancels
fingerprint generation, and attempts compatibility cleanup. The WebUI keeps
this control outside the configuration form and its busy state. New enable
requests acknowledge the current token, so an older request cannot override
a later stop. Importing, generating or applying a configuration does not enable
PIF. Recorded DenyList and SELinux-permission changes are restored only when
the current state still matches the engine's expected state. External edits are
preserved and incomplete cleanup can be retried. Magisk may terminate processes
when DenyList entries are restored under enforcement. Historical changes without
a record are never guessed. Persistent compatibility properties retain upstream
restoration: recorded original values may overwrite later edits, as disclosed in
the power control.

Already injected hooks require restarting PI processes; active mount namespaces,
ROM overlays and global properties require a device reboot. Disabling PIF restores
the original toolbox property-handler path, whose own switch and boot-hash behavior
remain independent. This control does not disable the keystore tools.

Configuration and optional keystore patch updates use the backend's
`integrity-transaction` contract (version 1). Backups and rendered candidates are
saved before writes; a private, synced journal records which participants may
have started. This is recoverable coordination, not a filesystem-wide atomic
transaction. After an interruption, the home page and Play Integrity page show
a recovery notice and require confirmation before restoring. New applications
are blocked until recovery completes. Normal failures trigger immediate rollback.
The keystore adapter owns its file mappings and restart requirements, including
when restoring a patch policy.

For recovery without the WebUI, use `duckd integrity-transaction status`, then
`duckd integrity-transaction recover --stdin-json` with `{"confirmed":true}` on
stdin after reviewing the journal. The journal lives under the toolbox data
root at `var/integrity-transaction/journal.json`; participant backup tokens are
retained there. Recovery accepts each prepared keystore file only if it matches
its original or proposed content, so partial publication can be reversed.
Unrelated edits cause refusal and leave the journal and backups available for
manual inspection. Do not delete these files to dismiss an unresolved update.
Filesystem durability still depends on the device and its storage implementation;
real power-loss and root-manager behavior need device validation.

**On a real KernelSU.** `scripts/device-test.sh <module zip>` prepares a disposable
userdebug emulator (or any device with root adbd) and runs `ui/e2e-device` there:
- KernelSU is late-loaded with `ksud late-load`, so no boot image is patched.
- The module and the Tricky Store, TEESimulator and OhMyKeymint releases are installed
  with `ksud module install`.
- For each keystore module, only that one is enabled and the device reboots.
- The tests drive the WebUI inside the KernelSU manager through Playwright's Android
  support. They read the files back over adb, and check each daemon's own log to confirm
  it accepted what the WebUI wrote.

## Acknowledgements

- Thanks to [gungnir-sanye](https://github.com/gungnir-sanye) for publishing
  [tee-attestation-extraction](https://github.com/gungnir-sanye/tee-attestation-extraction),
  whose research on Keybox and RKP/CDI extraction informed the TEE Attestation Extractor's
  scope and verification workflow. The offline implementation uses the workspace's
  existing libraries.
- The Tricky Store manager, its translations and the keybox repository protocol come from
  KOWX712's
  [Tricky Addon: Update Target List](https://github.com/KOWX712/Tricky-Addon-Update-Target-List)
  (Apache-2.0).
- The bundled software keybox is AOSP's reference attestation key
  (`system/keymaster/contexts/soft_attestation_cert.cpp`).
