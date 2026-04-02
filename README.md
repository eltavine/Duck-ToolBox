# Duck ToolBox

Duck ToolBox is a KernelSU module scaffold for modular Android utilities.

Current tool set:

- `RKP Workbench`: profile persistence, CSR generation, certificate fetch, `keybox.xml` export, and CSR verification
- `Device ID Provisioner`: Rust-based Qualcomm Keymaster device ID workbench with default auto-fill, advanced overrides, and provisioning reports

Core layout:

- `duckd/`: Rust backend
- `duckd/src/runtime/`: shared runtime concerns such as paths, profile storage, JSON envelopes, errors
- `duckd/src/features/rkp/`: RKP-specific feature modules
- `duckd/src/features/device_ids/`: built-in Device ID Provisioner feature, including default detection and Qualcomm Keymaster provisioning logic
- `duckd/src/shared/`: reusable parsing helpers such as Android XML decoding
- `ui/`: WebUI source
- `module/`: KernelSU module payload, including scripts, binaries, and built WebUI assets
- `module/webroot/`: built KernelSU WebUI assets
- `module/bin/duckctl.sh`: thin wrapper that forwards WebUI requests to `duckd`
- `module/service.sh`: boot-time repair for runtime directories and backend permissions

CLI shape:

- `duckd rkp profile show --json`
- `duckd rkp profile save --stdin-json --json`
- `duckd rkp profile clear --json`
- `duckd rkp info --json`
- `duckd rkp provision --json`
- `duckd rkp keybox --json`
- `duckd rkp verify <csr-file> --json`
- `duckd device-ids defaults --json`
- `duckd device-ids provision --stdin-json --json`
- `duckd artifacts list --json`

Runtime paths:

- Android module runtime data: `/data/adb/duck-toolbox/var/`
- Saved profile: `/data/adb/duck-toolbox/var/profile.toml`
- Saved secrets: `/data/adb/duck-toolbox/var/profile.secrets.toml`
- Outputs: `/data/adb/duck-toolbox/var/outputs/`
- Temporary files: `/data/adb/duck-toolbox/var/tmp/`
- Logs: `/data/adb/duck-toolbox/var/logs/`

Runtime robustness notes:

- `module/customize.sh` and `module/service.sh` repair runtime directories and sensitive file permissions at install time and on boot.
- Existing `var/` data is migrated out of the module directory into `/data/adb/duck-toolbox/var/`, so module updates no longer wipe saved profiles and generated files.
- Relative `var/...` paths now resolve inside the shared Duck ToolBox data directory on Android, while other relative paths still stay under the module root.
- RKP requests now validate required device fields before any network call, which makes profile mistakes fail early with clearer errors.
- Device ID provisioning now lives fully in `duckd/src/features/device_ids/`; the WebUI auto-fills common fields from the current Android runtime and keeps uncommon options under an advanced section.

Windows environment setup:

- PowerShell 7+
- Rust stable via `rustup`
- Rust target `aarch64-linux-android`
- `cargo-ndk`
- Node.js 22+
- `pnpm` 10+
- Android NDK r29 at `C:\Development\Android\NDK\android-ndk-r29`

Recommended one-time setup:

```powershell
rustup target add aarch64-linux-android
cargo install cargo-ndk --locked
npm install --global pnpm
```

Set the Android NDK location for the current PowerShell session:

```powershell
$env:ANDROID_NDK_ROOT = "C:\Development\Android\NDK\android-ndk-r29"
$env:ANDROID_NDK_HOME = $env:ANDROID_NDK_ROOT
$env:ANDROID_NDK = $env:ANDROID_NDK_ROOT
```

Recommended local build flow:

```powershell
cd duckd
cargo test --target x86_64-pc-windows-msvc

cargo ndk -t arm64-v8a build --release

cd ..\ui
pnpm install --frozen-lockfile
pnpm build

cd ..
# Package a local module zip after the Android backend build
pwsh .\scripts\package-module.ps1
```

Windows one-step build script:

```powershell
pwsh ./scripts/build.ps1
```

The script:

- uses `C:\Development\Android\NDK\android-ndk-r29` by default
- builds the Rust Android backend with `cargo ndk -t arm64-v8a build --release`
- installs WebUI dependencies with `pnpm install --frozen-lockfile`
- builds the WebUI into `module/webroot/`

Optional module packaging in the same run:

```powershell
pwsh ./scripts/build.ps1 -PackageModule
```

Build outputs:

- Rust Android binary: `duckd/target/aarch64-linux-android/release/duckd`
- WebUI bundle: `module/webroot/`
- Optional module archive: `dist/duck-toolbox-<version>.zip`

RKP note:

- The WebUI now opens in `HW Key + KDF Label` mode by default. A blank profile still stays logically unset until you provide real key material.

Release note:

- `update.json` intentionally ships with empty URLs. Fill in real release endpoints before publishing updates through KernelSU.
