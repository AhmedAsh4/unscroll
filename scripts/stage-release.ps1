# Unscroll V1 Windows release stager (Task 21).
#
# Fail-closed packaging input gate. Stages the release launcher APK built via
# `gradlew assembleRelease` (never committed) next to the pinned ADB runtime
# and verifies both sets before any Tauri bundle step:
#   desktop/src-tauri/resources/launcher/unscroll-launcher.apk
#   desktop/src-tauri/resources/launcher/unscroll-launcher.apk.sha256
#
# The sidecar carries the expected APK *signing identity* (lowercase hex64),
# which inspect.rs verifies against the bridge-reported `launcher_signing_sha256`;
# it is NOT the APK file hash. Supply it via -SigningSha256 (obtain with
# `apksigner verify --print-certs` and normalize to lowercase hex). Staging
# new APK bytes without an identity fails instead of inventing one.
#
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/stage-release.ps1 `
#     -LauncherApk launcher/app/build/outputs/apk/release/app-release.apk `
#     -SigningSha256 <lowercase-hex64>
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/stage-release.ps1 -VerifyOnly
#
# -ResourceDir overrides the resources root (default
# desktop/src-tauri/resources); it mirrors prepare-adb.ps1 -Destination for
# hermetic tests. No new dependencies: PowerShell + .NET only.

[CmdletBinding()]
param(
    [string]$LauncherApk,
    [string]$SigningSha256,
    [string]$ResourceDir,
    [switch]$VerifyOnly
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path $PSScriptRoot -Parent
Import-Module (Join-Path $PSScriptRoot 'prepare-adb.validation.psm1') -Force
if (-not $ResourceDir) { $ResourceDir = Join-Path $repoRoot 'desktop\src-tauri\resources' }
if (-not $LauncherApk) { $LauncherApk = Join-Path $repoRoot 'launcher\app\build\outputs\apk\release\app-release.apk' }
$adbDir = Join-Path $ResourceDir 'adb'
$launcherDir = Join-Path $ResourceDir 'launcher'
$stagedApkName = 'unscroll-launcher.apk'
$stagedApk = Join-Path $launcherDir $stagedApkName
$stagedSidecar = "$stagedApk.sha256"
$prepareAdb = Join-Path $PSScriptRoot 'prepare-adb.ps1'
$minApkBytes = 4
$maxApkBytes = 100 * 1024 * 1024

function Assert-SigningIdentity([string]$Value) {
    $normalized = $Value.Trim().ToLowerInvariant()
    if ($normalized -cnotmatch '^[0-9a-f]{64}$') {
        throw "Signing identity must be 64 lowercase hex characters (signing-sha256), got: $Value"
    }
    return $normalized
}

function Assert-StoredSigningIdentity([string]$Value) {
    # Stored sidecar records must already be lowercase hex64: the Rust
    # LauncherArtifact validator rejects anything else at runtime, so
    # verification fails here instead of blessing a pair the app would refuse.
    if ($Value -cnotmatch '^[0-9a-f]{64}$') {
        throw "Stored signing sidecar must be 64 lowercase hex characters (signing-sha256), got: $Value"
    }
}

function Assert-ApkBytes([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "Launcher package is missing: $Path" }
    $length = (Get-Item -LiteralPath $Path).Length
    if ($length -lt $minApkBytes -or $length -gt $maxApkBytes) {
        throw "Launcher package size $length bytes is outside the staged range 4..100MiB: $Path"
    }
    $stream = [IO.File]::OpenRead($Path)
    try {
        $magic = New-Object byte[] 4
        if ($stream.Read($magic, 0, 4) -ne 4 -or $magic[0] -ne 0x50 -or $magic[1] -ne 0x4b -or $magic[2] -ne 0x03 -or $magic[3] -ne 0x04) {
            throw "Launcher package is not a valid staged APK (missing PK03 04 magic): $Path"
        }
    } finally {
        $stream.Dispose()
    }
}

function Assert-AdbRuntime {
    if (-not (Test-Path -LiteralPath $adbDir -PathType Container)) {
        if ($VerifyOnly) { throw "ADB runtime is missing: $adbDir. Run scripts/prepare-adb.ps1 first." }
        & $prepareAdb -Destination $adbDir
        return
    }
    & $prepareAdb -VerifyOnly -Destination $adbDir
}

function Assert-LauncherClosed {
    if (-not (Test-Path -LiteralPath $stagedApk -PathType Leaf)) {
        throw 'The Unscroll launcher package is not staged with this installation. Stage it with scripts/stage-release.ps1 first.'
    }
    if (-not (Test-Path -LiteralPath $stagedSidecar -PathType Leaf)) {
        throw 'The Unscroll launcher signing sidecar is missing. Stage it with scripts/stage-release.ps1 first.'
    }
    $children = @(Get-ChildItem -LiteralPath $launcherDir -Force)
    $names = @($children | ForEach-Object { $_.Name })
    if ($children.Count -ne 2 -or ($names -cnotcontains $stagedApkName) -or ($names -cnotcontains "$stagedApkName.sha256")) {
        throw "The launcher resource set must contain only the staged launcher pair; got: $($names -join ', ')"
    }
    Assert-ApkBytes $stagedApk
    $identity = (Get-Content -LiteralPath $stagedSidecar -Raw).Trim()
    Assert-StoredSigningIdentity $identity
    $hash = (Get-FileHash -LiteralPath $stagedApk -Algorithm SHA256).Hash
    "Launcher APK SHA-256 (file bytes, for SHA256SUMS): $hash"
    "Launcher signing identity (sidecar): $identity"
}

Assert-AdbHost `
    -WindowsHost ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) `
    -OperatingSystem64Bit ([Environment]::Is64BitOperatingSystem) `
    -Architecture ([Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString())

if ($VerifyOnly) {
    if ($PSBoundParameters.ContainsKey('LauncherApk')) { throw 'LauncherApk cannot be used with VerifyOnly' }
    if ($PSBoundParameters.ContainsKey('SigningSha256')) { throw 'SigningSha256 cannot be used with VerifyOnly' }
    Assert-AdbRuntime
    Assert-LauncherClosed
    "Verified staged Windows release inputs at $ResourceDir"
    exit 0
}

if (-not (Test-Path -LiteralPath $LauncherApk -PathType Leaf)) {
    throw "Release launcher APK is missing: $LauncherApk. Build it with 'gradlew assembleRelease' (it is build output, NOT committed), then stage it with this script."
}
Assert-ApkBytes $LauncherApk
if (-not (Test-Path -LiteralPath $launcherDir -PathType Container)) {
    New-Item -ItemType Directory -Path $launcherDir -Force | Out-Null
}

if ($PSBoundParameters.ContainsKey('SigningSha256')) {
    $identity = Assert-SigningIdentity $SigningSha256
    Copy-Item -LiteralPath $LauncherApk -Destination $stagedApk -Force
    Set-Content -LiteralPath $stagedSidecar -Value $identity -NoNewline -Encoding Ascii
} else {
    $stagedValid = (Test-Path -LiteralPath $stagedApk -PathType Leaf) -and (Test-Path -LiteralPath $stagedSidecar -PathType Leaf)
    $sameBytes = $false
    if ($stagedValid) {
        $sourceHash = (Get-FileHash -LiteralPath $LauncherApk -Algorithm SHA256).Hash
        $destHash = (Get-FileHash -LiteralPath $stagedApk -Algorithm SHA256).Hash
        $sameBytes = $sourceHash -eq $destHash
        try {
            Assert-StoredSigningIdentity ((Get-Content -LiteralPath $stagedSidecar -Raw).Trim())
        } catch {
            $stagedValid = $false
        }
    }
    if (-not ($stagedValid -and $sameBytes)) {
        throw 'SigningSha256 is required when staging new APK bytes. Obtain the release signing identity with apksigner verify --print-certs, normalize it to lowercase hex, and pass -SigningSha256.'
    }
    Copy-Item -LiteralPath $LauncherApk -Destination $stagedApk -Force
}

Assert-AdbRuntime
Assert-LauncherClosed
"Staged Windows release inputs at $ResourceDir"
