# Task 21 release-stager tests (focused, fast, no network, no device).
#
# Fail-closed coverage for scripts/stage-release.ps1 plus a structural gate
# on desktop/src-tauri/tauri.conf.json (real JSON parsing) that complements
# desktop/src-tauri/tests/release_packaging.rs:
# - stage happy path + idempotent -VerifyOnly
# - missing source APK fails with the assembleRelease pointer
# - new APK bytes without -SigningSha256 fail (no placeholder identity)
# - tampered/extra/missing APK sidecar fails (case, length, alphabet, absent)
# - bad-magic and too-small APKs fail (LauncherArtifact rules)
# - extra files in the launcher or ADB runtime sets fail
# - missing ADB runtime fails under -VerifyOnly (full prepare needs network
#   and is never attempted here)
#
# All staging uses an isolated temp -ResourceDir; the repository's
# desktop/src-tauri/resources tree is never touched. Run from the repo root:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/stage-release.tests.ps1

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path $PSScriptRoot -Parent
$stager = Join-Path $PSScriptRoot 'stage-release.ps1'
$pinnedAdb = Join-Path $repoRoot 'desktop\src-tauri\resources\adb'
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('unscroll-stage-tests-' + [guid]::NewGuid().ToString('N'))
$validSigning = 'ab12cd34' * 8

function Assert-Fails([string]$Pattern, [scriptblock]$Action) {
    try {
        & $Action
    } catch {
        if ($_.Exception.Message -notmatch $Pattern) {
            throw "Expected failure '$Pattern', got: $($_.Exception.Message)"
        }
        "PASS: rejected $Pattern"
        return
    }
    throw "Expected failure '$Pattern', but the command succeeded"
}

function New-FakeApk([string]$Path, [int]$Size = 8192) {
    if ($Size -lt 4) { $Size = 4 }
    $bytes = New-Object byte[] $Size
    $bytes[0] = 0x50; $bytes[1] = 0x4b; $bytes[2] = 0x03; $bytes[3] = 0x04
    for ($i = 4; $i -lt $Size; $i++) { $bytes[$i] = [byte](0x41 + ($i % 26)) }
    [IO.File]::WriteAllBytes($Path, $bytes)
}

function New-IsolatedResources {
    $resources = Join-Path $testRoot ([guid]::NewGuid().ToString('N'))
    $adb = Join-Path $resources 'adb'
    New-Item -ItemType Directory -Path $adb -Force | Out-Null
    foreach ($name in @('adb.exe', 'AdbWinApi.dll', 'AdbWinUsbApi.dll')) {
        Copy-Item -LiteralPath (Join-Path $pinnedAdb $name) -Destination (Join-Path $adb $name)
    }
    return $resources
}

if (-not (Test-Path -LiteralPath $stager -PathType Leaf)) { throw "Release stager is missing: $stager" }
if (-not (Test-Path -LiteralPath $pinnedAdb -PathType Container)) { throw "Pinned ADB runtime is missing: $pinnedAdb" }
New-Item -ItemType Directory -Path $testRoot -Force | Out-Null
try {
    # Happy path: stage a fresh APK, then verify it idempotently.
    $resources = New-IsolatedResources
    $source = Join-Path $testRoot 'app-release.apk'
    New-FakeApk $source
    & $stager -LauncherApk $source -SigningSha256 $validSigning -ResourceDir $resources
    $stagedApk = Join-Path $resources 'launcher\unscroll-launcher.apk'
    $stagedSidecar = "$stagedApk.sha256"
    $sourceHash = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash
    $stagedHash = (Get-FileHash -LiteralPath $stagedApk -Algorithm SHA256).Hash
    if ($stagedHash -ne $sourceHash) { throw 'Staged APK bytes differ from the release APK' }
    if ((Get-Content -LiteralPath $stagedSidecar -Raw) -cne $validSigning) { throw 'Sidecar does not carry the exact signing identity' }
    & $stager -VerifyOnly -ResourceDir $resources
    'PASS: stage happy path plus idempotent verify'

    # New APK bytes without a signing identity fail (no placeholder identity).
    $rotated = Join-Path $testRoot 'rotated.apk'
    New-FakeApk $rotated
    $rotatedBytes = [IO.File]::ReadAllBytes($rotated)
    $rotatedBytes[$rotatedBytes.Length - 1] = $rotatedBytes[$rotatedBytes.Length - 1] -bxor 1
    [IO.File]::WriteAllBytes($rotated, $rotatedBytes)
    Assert-Fails 'SigningSha256 is required' { & $stager -LauncherApk $rotated -ResourceDir $resources }

    # Missing source APK points at the release build (never committed).
    Assert-Fails 'assembleRelease' { & $stager -LauncherApk (Join-Path $testRoot 'absent.apk') -SigningSha256 $validSigning -ResourceDir $resources }

    # Bad-magic and too-small APKs fail the LauncherArtifact rules.
    $badMagic = Join-Path $testRoot 'bad-magic.apk'
    New-FakeApk $badMagic
    $magicBytes = [IO.File]::ReadAllBytes($badMagic)
    $magicBytes[0] = [byte][char]'Z'
    [IO.File]::WriteAllBytes($badMagic, $magicBytes)
    Assert-Fails 'PK..04|magic' { & $stager -LauncherApk $badMagic -SigningSha256 $validSigning -ResourceDir $resources }
    $tiny = Join-Path $testRoot 'tiny.apk'
    [IO.File]::WriteAllBytes($tiny, @(0x50, 0x4b, 0x03))
    Assert-Fails 'size' { & $stager -LauncherApk $tiny -SigningSha256 $validSigning -ResourceDir $resources }

    # Malformed signing identities fail at stage time; uppercase input is
    # normalized so the stored sidecar is always lowercase hex64.
    $upperResources = New-IsolatedResources
    & $stager -LauncherApk $source -SigningSha256 ($validSigning.ToUpperInvariant()) -ResourceDir $upperResources
    $upperSidecar = Join-Path $upperResources 'launcher\unscroll-launcher.apk.sha256'
    if ((Get-Content -LiteralPath $upperSidecar -Raw) -cne $validSigning) { throw 'Uppercase signing input must normalize to the lowercase sidecar record' }
    'PASS: signing identity normalizes to lowercase hex64'
    Assert-Fails 'hex' { & $stager -LauncherApk $source -SigningSha256 ($validSigning.Substring(0, 63)) -ResourceDir (New-IsolatedResources) }
    Assert-Fails 'hex' { & $stager -LauncherApk $source -SigningSha256 ("g" + $validSigning.Substring(1)) -ResourceDir (New-IsolatedResources) }

    # Tampered/extra/missing sidecar state fails verification.
    $tampered = New-IsolatedResources
    & $stager -LauncherApk $source -SigningSha256 $validSigning -ResourceDir $tampered
    $tamperedApk = Join-Path $tampered 'launcher\unscroll-launcher.apk'
    [IO.File]::WriteAllText("$tamperedApk.sha256", $validSigning.ToUpperInvariant())
    Assert-Fails 'hex' { & $stager -VerifyOnly -ResourceDir $tampered }
    [IO.File]::WriteAllText("$tamperedApk.sha256", $validSigning.Substring(0, 63))
    Assert-Fails 'hex' { & $stager -VerifyOnly -ResourceDir $tampered }
    Remove-Item -LiteralPath "$tamperedApk.sha256" -Force
    Assert-Fails 'sidecar|not staged' { & $stager -VerifyOnly -ResourceDir $tampered }
    [IO.File]::WriteAllText("$tamperedApk.sha256", $validSigning)
    Remove-Item -LiteralPath $tamperedApk -Force
    Assert-Fails 'not staged|missing' { & $stager -VerifyOnly -ResourceDir $tampered }
    'PASS: tampered, extra, and missing sidecar/APK states fail closed'

    # Extra files in the launcher set fail verification.
    $extraLauncher = New-IsolatedResources
    & $stager -LauncherApk $source -SigningSha256 $validSigning -ResourceDir $extraLauncher
    [IO.File]::WriteAllText((Join-Path $extraLauncher 'launcher\notes.txt'), 'unexpected')
    Assert-Fails 'extra|only' { & $stager -VerifyOnly -ResourceDir $extraLauncher }

    # Extra files in the ADB runtime set fail verification (via prepare-adb).
    $extraAdb = New-IsolatedResources
    & $stager -LauncherApk $source -SigningSha256 $validSigning -ResourceDir $extraAdb
    [IO.File]::WriteAllText((Join-Path $extraAdb 'adb\unexpected.dll'), 'unexpected')
    Assert-Fails 'Runtime file set mismatch' { & $stager -VerifyOnly -ResourceDir $extraAdb }

    # Missing ADB runtime fails under -VerifyOnly instead of downloading.
    $noAdb = Join-Path $testRoot 'no-adb-resources'
    New-Item -ItemType Directory -Path (Join-Path $noAdb 'launcher') -Force | Out-Null
    Copy-Item -LiteralPath $stagedApk -Destination (Join-Path $noAdb 'launcher\unscroll-launcher.apk')
    Copy-Item -LiteralPath $stagedSidecar -Destination (Join-Path $noAdb 'launcher\unscroll-launcher.apk.sha256')
    Assert-Fails 'missing|Runtime directory' { & $stager -VerifyOnly -ResourceDir $noAdb }

    # -VerifyOnly rejects staging overrides (mirrors prepare-adb.ps1).
    Assert-Fails 'VerifyOnly' { & $stager -VerifyOnly -LauncherApk $source -ResourceDir $resources }
    Assert-Fails 'VerifyOnly' { & $stager -VerifyOnly -SigningSha256 $validSigning -ResourceDir $resources }

    # Release bundle surface: real JSON parsing of tauri.conf.json + capabilities.
    $conf = Get-Content -LiteralPath (Join-Path $repoRoot 'desktop\src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json
    if ($conf.productName -cne 'Unscroll') { throw 'productName must be Unscroll' }
    if ($conf.version -cne '0.1.0') { throw 'version must be 0.1.0' }
    if ($conf.identifier -cne 'org.unscroll.desktop') { throw 'identifier must be org.unscroll.desktop' }
    if ($conf.bundle.active -ne $true) { throw 'bundle.active must be true' }
    if (@($conf.bundle.targets).Count -ne 1 -or $conf.bundle.targets[0] -cne 'nsis') { throw 'bundle.targets must be exactly NSIS' }
    if ($conf.app.windows[0].width -ne 1050) { throw 'window width must be 1050' }
    if ($conf.app.windows[0].minWidth -ne 800) { throw 'window minWidth must be 800' }
    $resourceProps = @($conf.bundle.resources.PSObject.Properties)
    if ($resourceProps.Count -ne 2) { throw "bundle.resources must carry exactly 2 mappings, got $($resourceProps.Count)" }
    if ($conf.bundle.resources.'resources/adb/*' -cne 'adb/') { throw 'ADB resource mapping must be resources/adb/* -> adb/' }
    if ($conf.bundle.resources.'resources/launcher/*' -cne 'launcher/') { throw 'launcher resource mapping must be resources/launcher/* -> launcher/' }
    if (@($conf.bundle.externalBin).Count -ne 0) { throw 'externalBin must stay empty' }
    if ($conf.bundle.windows.nsis.startMenuFolder -cne 'Unscroll') { throw 'NSIS Start-menu folder must be Unscroll' }
    $rawConf = Get-Content -LiteralPath (Join-Path $repoRoot 'desktop\src-tauri\tauri.conf.json') -Raw
    foreach ($needle in @('unscroll_probe', 'target/', '"wix"', '"msi"', '"appimage"', '"dmg"')) {
        if ($rawConf.Contains($needle)) { throw "tauri.conf.json must not contain $needle" }
    }
    $caps = Get-Content -LiteralPath (Join-Path $repoRoot 'desktop\src-tauri\capabilities\default.json') -Raw | ConvertFrom-Json
    if (@($caps.permissions).Count -ne 1 -or $caps.permissions[0] -cne 'core:default') { throw 'capabilities must stay core:default only' }
    'PASS: release bundle surface (tauri.conf.json + capabilities)'

    'PASS: stage-release fail-closed verification'
} finally {
    if (Test-Path -LiteralPath $testRoot) { Remove-Item -LiteralPath $testRoot -Recurse -Force }
}
