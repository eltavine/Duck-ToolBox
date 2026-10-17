# Duck ToolBox

Duck ToolBox is a root module (KernelSU, APatch or Magisk) that puts several Android
attestation tools behind one WebUI. The backend is a Cargo workspace of single-purpose crates
compiled into one JSON CLI (`duckd`); the WebUI is a set of feature folders that the app
discovers at build time. Features only meet through versioned contracts, so each one can be
changed, removed or added on its own.

## Tools

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
  - Builds `AuthenticatedRequest` CSRs (DICE + COSE, per the AOSP
    `IRemotelyProvisionedComponent` HAL) and talks to the RKP server.
  - Verifies CSRs offline and exports `keybox.xml`, which the Tricky Store manager can
    install directly.
  - Device values are read from AOSP build properties.
- **Device RKP Issuer**: the same keybox export, but signed by the device itself instead of
  a seed you hold.
  - Asks this device's Qualcomm KeyMint TA for a `CSR_V2` over a P-256 key we generated,
    submits the result to Google's RKP server and exports `keybox.xml` with the private key.
  - Checks its prerequisites (Android arm64, root, `/dev/smcinvoke`,
    `libkeymasterdeviceutils.so`, Qualcomm SoC) before it sends anything.
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
- **Device ID Provisioner**: writes attestation device IDs into the Qualcomm Keymaster
  trusted app through `libQSEEComAPI.so`, with a dry-run mode.
- **Module & updates**: version and device summary, stable and canary updates with
  changelog, output files, command history, uninstall and reboot.

Every tool is translated into 14 languages: English, Simplified and Traditional Chinese,
Japanese, Korean, Spanish, French, German, Italian, Brazilian Portuguese, Russian, Arabic,
Vietnamese and Indonesian. Tricky Addon's other translations cover the strings the Tricky
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
   `duckd()` with zod schemas, a `locales/<language>.json` for each of the 14 languages
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
The manual **Stable** workflow reuses the CI checks and builds, preserves `version`
and `versionCode` from `module/module.prop`, publishes `v<version>` as the latest
non-prerelease release, and sends the same ZIP to Telegram with a `stable` label.
Before running it on `main`, increase `versionCode` beyond all accepted releases and
update `update.json` to match the version and `duck-toolbox-<version>.zip` asset.

**KernelSU Modules repository.** The root `module.yaml` supplies the module listing
metadata and selects `duck-toolbox-*.zip` release assets. Published ZIPs contain
`module.prop` at their root; the current builds target Android API 26+ on arm64-v8a.
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
```

Commands that take input read JSON from stdin with `--stdin-json`.

`rkp-device status` identifies the SoC from manufacturer aliases, SoC part numbers,
hardware properties and the kernel's [SoC family/machine attributes](https://www.kernel.org/doc/rustdoc/latest/kernel/soc/struct.Attributes.html). It reports the raw
evidence and distinguishes Qualcomm, another vendor, unknown and conflicting information.
Unknown or conflicting information blocks issuance rather than guessing. These local
checks do not establish firmware ABI compatibility or server acceptance. The status format
uses feature contract 2; update the backend and WebUI together.

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
- Node.js 24 and pnpm 12 (`corepack enable` picks the version from `ui/package.json`).
  Type checking runs the TypeScript 7 engine through typescript-native-bridge, because
  stock TypeScript 7 no longer has the API vue-tsc needs.

```bash
cargo xtask line-limit
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

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
