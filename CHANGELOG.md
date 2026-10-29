# Changelog

## Unreleased

### Linux environments

- Import trusted ARM64 rootfs archives with inspected SHA-256 verification, bounded
  extraction, asynchronous progress, cancellation and private staging.
- Manage private mounts, terminal entry, foreground services, DNS, bounded logs and
  optional startup after boot. Use PID namespaces or cgroup v2 to track guest processes.
- Confirm changes against the current configuration revision and preserve data when
  termination cannot be verified. Keep all 16 locales and RTL layouts aligned with
  existing interface components.

### Kernel modules

- Reject duplicate or unreadable ELF metadata sections, and mark inferred or repeated
  module names explicitly during inspection.
- Validate load parameters by UTF-8 bytes before confirmation, with localized field errors
  and persistent inspection progress/error feedback across 16 locales.
- Protect the `kernelsu` root core from unloading, reject unknown holder state, and
  expose protection reasons consistently in the backend and all 16 interface locales.
- Use explicit confirmation action labels and isolate module names in RTL sentences;
  preserve confirmation content through close transitions and label module taint accurately.
- Add a separate Linux kernel module browser with loaded/sysfs-only state, references,
  dependents, runtime metadata and read-only parameters.
- Browse standard Android module directories and inspect uncompressed `.ko` ELF metadata.
- Add confirmed, serialized runtime load/unload operations with inspected-file digest checks,
  dependency checks, kernel-enforced restrictions and explicit unknown-outcome timeout reports.
- Match Android kernel procfs variants and keep release-prefix comparison advisory for
  symbol-versioned modules. Check ELF class/byte order and preserve parameter whitespace.
- Validate worker protocol/results, bound process cleanup, and retain partial read failures.
- Add localized runtime states, isolated technical strings, persistent operation feedback,
  keyboard focus restoration and layouts that adapt to larger text across 16 locales.

## 0.2.1-Stable - 2026-10-26

### Play Integrity

- Fetch selectable Pixel release fingerprints from Google build metadata, with
  Pixel 6a selected by default. Preview candidates and save their source metadata
  without applying them or enabling the engine. Leave unknown patch dates unset.
- Add quick controls for Build, property and Play Store fingerprint overrides.
  Explain the Play Store SDK compatibility mode and its precedence over fingerprints.
- Preview system, vendor and boot patch policies separately while retaining
  unselected partition settings and backend-specific formats.
- Persist process restart and device reboot requirements across page reloads.
  Keep independent keystore recovery available when an activation marker fails.
- Compare process-local Build and property snapshots with the saved configuration,
  matching revision, boot identity, PID and process start time. Import decoded
  integrity reports as unverified summaries; missing labels remain unknown.
- Extend configuration, recovery, JNI and mobile browser regression coverage,
  including narrow English, Chinese and right-to-left layouts. Local diagnostics
  and tests do not establish a Google Play Integrity verdict.

### Translations

- Add Ukrainian and Polish translations and localize the new integrity controls.

## 0.2.0-Stable - 2026-10-24

### Integrity and keystore compatibility

- Keep OEM lock-state and AVB property changes in their appropriate boot stages,
  including when Play Integrity is disabled, unconfigured or falls back after a failure.
- Validate SHA-256/SHA-512 boot hashes against backend requirements before saving.
  Preserve independent OhMyKeymint trust settings when only the prop handler changes.
- Serialize keystore configuration writes, restore failed updates, retain unknown
  default and per-app policy fields, and reject stale configuration or backend selections.
- Keep unsaved property and policy edits across background refreshes. Add explicit
  property reload and report settings that require a reboot.
- Align module disable/remove handling across the CLI, shell and native companion.
  Reject invalid compatibility options consistently without changing saved enable intent.
- Respect separate system, vendor and boot patch values when preparing optional
  keystore synchronization. Retain explicit configurations and skip incompatible
  single-date updates.

### Device provisioning

- Add MediaTek SoC detection, AIDL V1 RKP support and TrustKernel device-ID
  provisioning while retaining Qualcomm-specific defaults and backend selection.
- Refuse contradictory platform evidence and retain diagnostics for failed native
  provisioning. Firmware support and successful hardware issuance remain device-specific.

### Verification

- Extend configuration rollback, lifecycle, native/JVM, contract and mobile WebUI
  regression coverage, including API 36 AVD checks with KernelSU 3.3.0 and
  OhMyKeymint 1.2.0-b12557c.
- Local tests and emulator behavior do not establish Google Play Integrity verdicts
  or physical-device TEE, fingerprint and display compatibility.

## 0.1.8-stable - 2026-10-24

### Play Integrity

- Integrate a separately built Play Integrity Fork-derived engine with an opt-in
  configuration workflow, generation of Pixel Canary candidates, compatibility
  controls, diagnostics and history. Candidate generation and local configuration
  do not guarantee any Google Play Integrity verdict.
- Preview changes before applying configuration or optional keystore patch policies.
  Support structured field editing, raw import/export, saved profiles, migration,
  rollback, and confirmation before recovering an interrupted update.
- Make engine activation and emergency disable independent from configuration reads,
  including during errors and pending cleanup. Restore managed state when disabling.
- Improve phone-sized layouts: consistent navigation, panels, editor controls and
  preview alignment. Preserve drafts while switching tabs and correct Back behavior.

### Reliability and updates

- Paginate command history and diagnose empty backend responses.
- Handle different fingerprint-generator output paths and improve its diagnostics.
- Fall back to an English release download link when a Telegram ZIP exceeds the
  Bot API's upload limit.

### Licensing and verification

- Keep the derived engine under GPLv3 with upstream notices, and retain the
  independently implemented management interface under MIT.
- Link to the immutable build commit for corresponding source rather than
  embedding a duplicate source archive in the module.
- Add independent engine compilation, packaging checks, contract tests and
  mobile WebUI regression coverage to CI.

## 0.1.7-stable - 2026-10-19

### Tricky Store manager

- Accept EC-only keyboxes for OhMyKeymint while retaining certificate-chain and
  certificate-count validation for every supplied key.
- Preserve legacy and modern Xposed module detection with zip 9 through exact
  central-directory path lookups, including APKs with non-UTF-8 filenames.

### Dependencies

- Upgrade zip to 9.0.4 and refresh Rust and WebUI dependencies and lockfiles.

## 0.1.6-stable - 2026-10-18

### WebUI

- Run shell commands through the host's queued job API when available, so slow
  commands no longer block return navigation. Preserve output, exit status and
  compatibility with hosts that only provide `exec`.
- Stop restarting failed automatic update checks on each return to the home page;
  keep manual retries and channel changes available. Read the fixed hosting module
  identity once per WebUI session.
- Reduce target-list reconstruction overhead with shallow query snapshots, stable
  drafts and lightweight selection indicators. Measure new virtual rows through
  ResizeObserver to avoid repeated forced layout, including with enlarged text.
- Clean up command callbacks on completion and launch failures, including hosts
  that report a companion error after a nonzero exit.

## 0.1.5-stable - 2026-10-18

### WebUI

- Virtualize target and system-app lists while preserving search, bulk selection,
  keyboard focus and per-app controls. Batch synchronous package metadata lookups
  and stop queued lookups when their query is cancelled.
- Preserve edits made during target saves, update the saved baseline without an
  extra package-manager query, and guard overlapping writes.
- Keep RKP navigation available during loading, prevent overlapping profile
  operations, and discard results for changed inputs.
- Fix cached Device ID form initialization, language-switch races, cancelled file
  reads and malformed repository messages. Bound uploads and translation requests.
- Adapt buttons, tabs, forms, menus, dialogs and sheets to narrow screens, landscape
  and enlarged text. Wrap technical values and application names, improve supporting
  text size and contrast, and respect display safe areas.

## 0.1.4-stable - 2026-10-17

### RKP Workbench

- Add **Import DICE materials** as the main workbench entry, with separate offline
  validation, diagnostic CSR construction and online issuance commands. An existing CSR
  is not required; device collection and firmware patching are not implemented here.
- Preserve the imported chain, match explicit signing material to its leaf key, and
  construct a new CSR v3 using a fresh server challenge and newly generated attestation key.
- Share issuance and export between imported material and the labelled experimental
  degenerate-chain mode. Remove implicit offline fallback and successful error results.
- Select returned chains by public key, verify certificate signatures and constraints,
  and publish private keyboxes atomically after XML readback checks. JSON results contain
  paths and summaries instead of private XML. Installation remains independent.
- Bump the workbench feature contract to 2. Old profiles retain their seed derivation;
  material import does not overwrite profiles or an installed keybox.
- Accepts the integer DICE mode allowed by the `android.14` profile.
- Verification uses generated synthetic keys and a local simulated server. Real-material
  production acceptance and successful production export remain unverified for this path.

### Device RKP Issuer
- Automatic mode tries V2, AIDL V1 and native V1 in order, skipping unavailable methods.
  Any issuance or validation failure advances to the next method with a fresh challenge;
  failures are aggregated only after every method fails. Successful results include an
  attempt history. Local file writes happen after certificate validation succeeds.
- Adds the `v1-native` backend for vendor command `0x311e`. Native V1 reuses the
  V1 CSR assembler, structural checks and isolated worker, and does not require a system
  AIDL APK at runtime. Firmware and server acceptance remain unverified.
- Reports protocol and backend separately, exposes native V1 resource availability,
  and uses feature contract 5 across the CLI and WebUI.
- Adds the V1 AIDL signing oracle for Qualcomm PureSoft firmware with a zero HBK,
  adapted from LazyBonesLZY/rkp-v1-oracle under its MIT license.
- Adds automatic and explicit V1/V2 protocol selection.
- Selects the full EEK chain for the HAL's reported curve and uses the device's AIDL
  proxy in an isolated process, with bounded exchange files and timeout cleanup.
- Reports V1 as structurally checked; encrypted signing evidence is verified by the
  provisioning server. V2 retains local signature verification.
- Cross-checks Qualcomm aliases (including QTI/QCOM), SoC part numbers, hardware properties
  and kernel SoC information. Unknown and conflicting evidence have separate blockers;
  empty properties no longer imply support. The WebUI shows the evidence and keeps firmware
  and server compatibility unverified.
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

### WebUI
- Keeps long error details, links and code within the viewport.

### Command wrapper
- Preserves backend failure exit codes and tries fallback binaries only after execution
  failures (`126` or `127`).

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
