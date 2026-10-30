[CmdletBinding()]
param(
  [string]$NdkRoot = $(if ($env:ANDROID_NDK_HOME) { $env:ANDROID_NDK_HOME } else { "C:\Development\Android\NDK\android-ndk-r30" }),
  [string]$SdkRoot = $(if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { $env:ANDROID_SDK_ROOT }),
  [ValidateSet("debug", "release")]
  [string]$Profile = "release",
  [string]$AndroidAbi = "arm64-v8a",
  [switch]$SkipRust,
  [switch]$SkipWeb,
  [switch]$PackageModule
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = Split-Path -Parent $PSScriptRoot
$uiDir = Join-Path $repoRoot "ui"
$packageScript = Join-Path $PSScriptRoot "package-module.ps1"
$toolchainBin = Join-Path $NdkRoot "toolchains\llvm\prebuilt\windows-x86_64\bin"
$binaryRelativePath = "target\aarch64-linux-android\$Profile\duckd"
$binaryPath = Join-Path $repoRoot $binaryRelativePath

function Write-Step([string]$Message) {
  Write-Host ""
  Write-Host "==> $Message" -ForegroundColor Cyan
}

function Invoke-Step([string]$Message, [scriptblock]$Action) {
  Write-Step $Message
  & $Action
}

function Assert-Command([string]$Name, [string]$InstallHint) {
  if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
    throw "$Name is not installed or not on PATH. $InstallHint"
  }
}

function Invoke-Checked([scriptblock]$Action, [string]$FailureMessage) {
  & $Action
  if ($LASTEXITCODE -ne 0) {
    throw $FailureMessage
  }
}

if (-not $SkipRust) {
  if (-not $SdkRoot -or -not (Test-Path $SdkRoot)) {
    throw "Set ANDROID_HOME or pass -SdkRoot with Android SDK Build Tools 36.0.0 installed."
  }
  $env:ANDROID_HOME = $SdkRoot
  Assert-Command "cargo" "Install Rust via rustup before running this script."
  Assert-Command "rustup" "Install Rust via rustup before running this script."

  if (-not (Test-Path $NdkRoot)) {
    throw "Android NDK directory not found at '$NdkRoot'."
  }

  if (-not (Test-Path $toolchainBin)) {
    throw "Android NDK LLVM toolchain not found at '$toolchainBin'."
  }

  $env:ANDROID_NDK_ROOT = $NdkRoot
  $env:ANDROID_NDK_HOME = $NdkRoot
  $env:ANDROID_NDK = $NdkRoot

  $pathEntries = $env:Path -split ";"
  if ($pathEntries -notcontains $toolchainBin) {
    $env:Path = "$toolchainBin;$env:Path"
  }

  Invoke-Step "Checking cargo-ndk" {
    & cargo ndk --version *> $null
    if ($LASTEXITCODE -ne 0) {
      throw "cargo-ndk is required. Install it with: cargo install cargo-ndk --locked"
    }
  }

  Invoke-Step "Ensuring Rust target aarch64-linux-android is installed" {
    $installedTargets = rustup target list --installed
    if ($installedTargets -notcontains "aarch64-linux-android") {
      Invoke-Checked { rustup target add aarch64-linux-android } "Failed to add Rust target aarch64-linux-android."
    }
  }

  Invoke-Step "Building Rust backend with cargo-ndk ($AndroidAbi, $Profile)" {
    Push-Location $repoRoot
    try {
      $cargoArgs = @("ndk", "-t", $AndroidAbi, "build", "--package", "duckd")
      if ($Profile -eq "release") {
        $cargoArgs += "--release"
      }
      Invoke-Checked { & cargo @cargoArgs } "Rust Android build failed."
    }
    finally {
      Pop-Location
    }
  }

  if (-not (Test-Path $binaryPath)) {
    throw "Expected Android backend binary was not produced at '$binaryPath'."
  }

  Write-Host "Rust binary: $binaryPath" -ForegroundColor Green
}

if (-not $SkipWeb) {
  Assert-Command "pnpm" "Install pnpm 12 (run corepack enable) before running this script."

  Invoke-Step "Installing WebUI dependencies" {
    Push-Location $uiDir
    try {
      Invoke-Checked { & pnpm install --frozen-lockfile } "pnpm install failed."
    }
    finally {
      Pop-Location
    }
  }

  Invoke-Step "Building WebUI" {
    Assert-Command "git" "Git is required to pin corresponding source to this build."
    $sourceCommit = (& git -C $repoRoot rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0 -or $sourceCommit -notmatch '^[0-9a-f]{40}$') {
      throw "Could not resolve the exact source commit."
    }
    Push-Location $uiDir
    $previousSourceCommit = $env:VITE_PIF_SOURCE_COMMIT
    $previousFakerCommit = $env:VITE_DEVICE_FAKER_SOURCE_COMMIT
    try {
      $env:VITE_PIF_SOURCE_COMMIT = $sourceCommit
      $env:VITE_DEVICE_FAKER_SOURCE_COMMIT = $sourceCommit
      Invoke-Checked { & pnpm build } "WebUI build failed."
    }
    finally {
      if ($null -eq $previousSourceCommit) {
        Remove-Item Env:VITE_PIF_SOURCE_COMMIT -ErrorAction SilentlyContinue
      } else {
        $env:VITE_PIF_SOURCE_COMMIT = $previousSourceCommit
      }
      if ($null -eq $previousFakerCommit) {
        Remove-Item Env:VITE_DEVICE_FAKER_SOURCE_COMMIT -ErrorAction SilentlyContinue
      } else {
        $env:VITE_DEVICE_FAKER_SOURCE_COMMIT = $previousFakerCommit
      }
      Pop-Location
    }
  }
  $webrootIndex = Join-Path $repoRoot "module\webroot\index.html"
  if (-not (Test-Path $webrootIndex)) {
    throw "Expected WebUI output was not produced at '$webrootIndex'."
  }
  Write-Host "WebUI output: $webrootIndex" -ForegroundColor Green
}

Invoke-Step "Building the independent Play Integrity engine" {
  Invoke-Checked { python (Join-Path $PSScriptRoot "build-play-integrity.py") --sdk $SdkRoot } "Play Integrity engine build failed."
}

if ($PackageModule) {
  Invoke-Step "Packaging KernelSU module zip" {
    & $packageScript -BinaryPath $binaryRelativePath
    if ($LASTEXITCODE -ne 0) { throw "Module packaging failed." }
  }
}
Write-Host ""
Write-Host "Build completed successfully." -ForegroundColor Green
