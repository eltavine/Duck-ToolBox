# Changelog

## Unreleased

### TEE Attestation Extractor
- New offline tool for extracting Keybox XML and PEM material from existing captures and
  verifying private-key correspondence, certificate signatures and chain completeness.
- Verifies candidate RKP identity material against the signing leaf of a locally verified
  CSR, with separate Ed25519-seed, P-256-seed and P-256-scalar modes and raw capture offsets.
- Returns summaries without exposing private material. Explicit exports use private output
  directories and owner-only files; mismatched identity material cannot be exported.
- Adds binary uploads, device file paths, manager installation through an extension point,
  and translations in all 14 languages. Device-side acquisition is not included.
- Local checks do not establish root trust, revocation status, registration or server acceptance.
  Supported private keys are EC P-256/P-384 and RSA 2048–8192; RSA-1024 is unsupported.
- Verifies private-key PEM labels against their DER encoding. Inline inputs are limited
  to 32 KiB per file to fit shell transport; device file paths accept up to 256 KiB.
- Finds XML document boundaries through parsed tags, accepts closing-tag whitespace,
  and rejects illegal XML characters before exporting material.
- Rejects trailing data in CSR CBOR items, including embedded signed payloads, protected
  headers, DICE subject keys and configuration descriptors.
- Resolves same-name certificate issuers by signature and handles self-issued key rotation
  certificates without treating them as self-signed roots.
- Ignores root-tag examples in XML comments outside the captured document.

### Command wrapper
- Preserves backend failure exit codes and tries fallback binaries only after execution
  failures (`126` or `127`).

### Device RKP Issuer
- Cross-checks Qualcomm aliases (including QTI/QCOM), SoC part numbers, hardware properties
  and kernel SoC information. Unknown and conflicting evidence have separate blockers;
  empty properties no longer imply support. The WebUI shows the evidence and keeps firmware
  and server compatibility unverified. Feature contract 2 prevents mixing older status data
  with the updated UI.
- Recognizes QTI as Qualcomm and gives the isolated worker vendor library search paths,
  retaining the default namespace's system dependency links for libraries such as libcrypto.
- Preserves worker error types and accepts bare as well as field-61-wrapped CSRs.
- Bounds exchange-file reads, rejects nonregular files and symlinks, and requires exactly
  one nonempty certificate chain for the requested key.
- Reserves private output directories atomically, removes partial runs on write failure,
  and saves keyboxes and private keys with mode `0600`, including explicit output copies.
- Uses the prerequisite report's device fingerprint consistently; an explicit fingerprint
  can satisfy a missing fingerprint without bypassing other prerequisites.
- Corrects the key export description: the exported keybox includes a software-generated
  private key; this path does not extract a private key from the TEE.

## 0.1.3-stable - 2026-10-17

### Device RKP Issuer
- New tool: signs a P-256 key we generated with this device's Qualcomm KeyMint TA
  (`CSR_V2`, command `0x3122` over `libkeymasterdeviceutils.so`) and submits the resulting
  `AuthenticatedRequest` to Google's RKP server, then exports `keybox.xml` plus its private
  key. The request does not extract a private key from the TEE.
- Warns twice before it can run, and the backend refuses a run without
  `--acknowledge-limitations`: only some devices work, and only RKP certificates can be
  issued.
- Checks Android arm64, root, `/dev/smcinvoke`, `libkeymasterdeviceutils.so` and a Qualcomm
  SoC first, and reports exactly which prerequisite is missing.
- Validates the device's CSR (DICE chain, signature, challenge, `keysToSign`) and binds the
  issued certificate chain to the generated key before writing anything. The chain must also
  be one leaf-to-root path with no certificate left off it. These local shape and key
  checks do not establish certificate trust, revocation status or server acceptance.

## 0.1.2-stable - 2026-10-11

### Tricky Store manager
- Complete port of Tricky Addon (Update Target List):
  - Target list with modes, per-app policy, search, bulk selection, DenyList, unnecessary
    apps, system apps and auto-add.
  - Keyboxes from AOSP, "unknown", a local file, the keybox repository and custom providers,
    with import and export.
  - Prop settings, default policy with *Today*, help, keyboard shortcuts and its 23
    translations, including right-to-left languages.
- One manager for Tricky Store (`config.ini` and legacy `target.txt`), TEESimulator
  (`config.json`) and OhMyKeymint (`config.toml` + `injector.toml`).
  - The validation rules follow each module's own sources.
  - Unknown sections, comments and keys are preserved.
- Optional WebUI entry on the keystore module (plus the action button on Magisk), and the
  TSupport-Advance auto-target stop flag.
- Tricky Addon's in-app translation update, translation guide and Telegram link.
- Checked against the real modules on a KernelSU device, which found and fixed:
  - OhMyKeymint v1.2.0 rejected our `injector.toml` (an unknown `version` key) while the
    WebUI reported success. `[scoop.<package>]` tables are now read and written like
    OhMyKeymint's own parser does.
  - The "unknown" keybox now gives each key a two-certificate chain. Tricky Addon's
    single-certificate keybox only loads on Tricky Store: TEESimulator and OhMyKeymint
    refuse it.
  - TEESimulator accepts EC-only keyboxes, such as the RKP workbench exports.
  - Saving tells when OhMyKeymint needs a restart to apply a change.

### WebUI
- Rebuilt on Vue 3.5, Vite 8, Tailwind CSS 4 and shadcn-vue.
  - A black and white theme, light or dark, where color only marks errors, success and
    warnings.
  - Designed for phones along Material 3 Expressive:
    - 48dp touch targets and a visible response to every press.
    - Round buttons that square up while pressed, segmented lists, and spring motion.
    - A large title that collapses into the 64dp app bar, for one-handed reach.
    - Tonal gray surfaces instead of borders and shadows, with contrast checked
      against WCAG.
  - Menus, tabs and button groups mirror in right-to-left languages too.
  - Follows the KernelSU manager's window insets.
  - The Android back gesture closes dialogs and sheets.
- Every tool in 14 languages: English, Simplified and Traditional Chinese, Japanese, Korean,
  Spanish, French, German, Italian, Brazilian Portuguese, Russian, Arabic, Vietnamese and
  Indonesian.
- Help: long links and code wrap inside the page, and two slips in upstream translations
  (a link missing its parenthesis, a code span closed with a quote) no longer show raw
  Markdown.
- Every tool is a self-contained feature folder, discovered automatically.
  - Features plug into each other only through extension points.
  - Every backend response is validated against a schema.
- A tool whose backend feature is missing or on another contract version is disabled
  instead of misreading data.
- Unit tests (Vitest) and end-to-end tests (Playwright against a mock KernelSU host) with
  light, dark, right-to-left and Chinese screenshots.
- Contract tests run every API call through `duckctl.sh` and a real `duckd` build and
  parse the results with the WebUI's schemas.
- Device tests (`scripts/device-test.sh`) drive the WebUI inside the real KernelSU manager
  with each keystore module installed.
- Loads as seven files instead of about eighty. The KernelSU manager reads each request
  through a root shell, so this halved the load time on a slow device.
- Type-checked on the TypeScript 7 engine through typescript-native-bridge.

### Backend
- `duckd` builds its CLI from a feature registry. `duckd features` reports the compiled-in
  features, and each feature can be built alone.
- New `system` feature: module and device summary, stable and canary updates with changelog,
  output files, command history, uninstall and reboot.
- RKP profiles read their device defaults from AOSP build properties.
- Updated RustCrypto to its current generation (p256/p384 0.14, ed25519/x25519 3, aes-gcm
  0.11, x509-cert 0.3). Keys derived from saved seeds are pinned by regression tests.

### Module
- Scripts follow the KernelSU module guide:
  - Late-load support (`late-load.sh`).
  - A late prop pass after boot for values the system sets late.
  - The module description goes into KernelSU's temporary config.
- Magisk runs the boot-completed work from `service.sh`.
- The prop handler adds `ro.boot.vbmeta.size`, and applies the boot hash even when the
  handler is disabled, as upstream does.
- Uninstalling removes entry links, flags and saved data.

### Tooling
- CI fails when any source file in any language exceeds 600 lines.
- CI: NDK r30, Node 24, pnpm 12, WebUI lint/format/unit/E2E, per-feature builds.
- Canary artifacts are the installable module directory.
- Dependabot opens one pull request per major update.
