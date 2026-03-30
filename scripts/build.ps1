[CmdletBinding()]
param(
  [string]$NdkRoot = "C:\Development\Android\NDK\android-ndk-r29",
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
$duckdDir = Join-Path $repoRoot "duckd"
$uiDir = Join-Path $repoRoot "ui"
$packageScript = Join-Path $PSScriptRoot "package-module.ps1"
$toolchainBin = Join-Path $NdkRoot "toolchains\llvm\prebuilt\windows-x86_64\bin"
$binaryRelativePath = "duckd\target\aarch64-linux-android\$Profile\duckd"
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
    Push-Location $duckdDir
    try {
      $cargoArgs = @("ndk", "-t", $AndroidAbi, "build")
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
  Assert-Command "pnpm" "Install pnpm 10+ before running this script."

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
    Push-Location $uiDir
    try {
      Invoke-Checked { & pnpm build } "WebUI build failed."
    }
    finally {
      Pop-Location
    }
  }

  $webrootIndex = Join-Path $repoRoot "webroot\index.html"
  if (-not (Test-Path $webrootIndex)) {
    throw "Expected WebUI output was not produced at '$webrootIndex'."
  }

  Write-Host "WebUI output: $webrootIndex" -ForegroundColor Green
}

if ($PackageModule) {
  Invoke-Step "Packaging KernelSU module zip" {
    & $packageScript -BinaryPath $binaryRelativePath
    if ($LASTEXITCODE -ne 0) {
      throw "Module packaging failed."
    }
  }
}

Write-Host ""
Write-Host "Build completed successfully." -ForegroundColor Green
