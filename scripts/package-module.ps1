param(
  [string]$BinaryPath = "duckd\target\aarch64-linux-android\release\duckd",
  [string]$DistDir = "dist"
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

$repoRoot = Split-Path -Parent $PSScriptRoot
$modulePropPath = Join-Path $repoRoot "module.prop"
$webrootIndex = Join-Path $repoRoot "webroot\index.html"
$binaryFullPath = Join-Path $repoRoot $BinaryPath
$distRoot = Join-Path $repoRoot $DistDir
$stageRoot = Join-Path $distRoot "stage"

if (-not (Test-Path $binaryFullPath)) {
  throw "Android backend binary not found at '$binaryFullPath'. Build it first or pass -BinaryPath."
}

if (-not (Test-Path $webrootIndex)) {
  throw "WebUI build output is missing at '$webrootIndex'. Run 'pnpm build' in ui first."
}

$versionLine = Get-Content $modulePropPath | Where-Object { $_ -match '^version=' } | Select-Object -First 1
if (-not $versionLine) {
  throw "Could not read version from '$modulePropPath'."
}

$version = $versionLine.Split('=', 2)[1].Trim()
$archivePath = Join-Path $distRoot "duck-toolbox-$version.zip"
$stageRootWithSeparator = "$stageRoot\"

if (Test-Path $stageRoot) {
  Remove-Item $stageRoot -Recurse -Force
}

New-Item -ItemType Directory -Force -Path $stageRoot | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $stageRoot "bin") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $stageRoot "webroot") | Out-Null
New-Item -ItemType Directory -Force -Path $distRoot | Out-Null

$requiredFiles = @(
  "module.prop",
  "README.md",
  "LICENSE",
  "customize.sh",
  "service.sh",
  "skip_mount"
)

foreach ($file in $requiredFiles) {
  Copy-Item (Join-Path $repoRoot $file) (Join-Path $stageRoot $file) -Force
}

$optionalFiles = @("update.json")
foreach ($file in $optionalFiles) {
  $source = Join-Path $repoRoot $file
  if (Test-Path $source) {
    Copy-Item $source (Join-Path $stageRoot $file) -Force
  }
}

Copy-Item (Join-Path $repoRoot "bin\duckctl.sh") (Join-Path $stageRoot "bin\duckctl.sh") -Force
Copy-Item $binaryFullPath (Join-Path $stageRoot "bin\duckd") -Force
Copy-Item (Join-Path $repoRoot "webroot\*") (Join-Path $stageRoot "webroot") -Recurse -Force

if (Test-Path $archivePath) {
  Remove-Item $archivePath -Force
}

$archive = $null
try {
  $archive = [System.IO.Compression.ZipFile]::Open(
    $archivePath,
    [System.IO.Compression.ZipArchiveMode]::Create
  )

  foreach ($file in Get-ChildItem -Path $stageRoot -Recurse -File | Sort-Object FullName) {
    $relativePath = $file.FullName.Substring($stageRootWithSeparator.Length).Replace('\', '/')
    [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
      $archive,
      $file.FullName,
      $relativePath,
      [System.IO.Compression.CompressionLevel]::Optimal
    ) | Out-Null
  }
}
finally {
  if ($null -ne $archive) {
    $archive.Dispose()
  }
}

$archive = $null
try {
  $archive = [System.IO.Compression.ZipFile]::OpenRead($archivePath)
  $invalidEntries = @($archive.Entries | Where-Object { $_.FullName -like '*\*' } | Select-Object -ExpandProperty FullName)
  if ($invalidEntries.Count -gt 0) {
    throw "Archive contains Windows-style path separators: $($invalidEntries -join ', ')"
  }
}
finally {
  if ($null -ne $archive) {
    $archive.Dispose()
  }
}

Write-Host "Module archive created at $archivePath"
