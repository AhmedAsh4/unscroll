param(
    [Parameter(Mandatory = $true)]
    [string]$ArchivePath
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path $PSScriptRoot -Parent
$prepare = Join-Path $PSScriptRoot 'prepare-adb.ps1'
$validationModule = Join-Path $PSScriptRoot 'prepare-adb.validation.psm1'
$license = Join-Path $repoRoot 'third_party\adb\LICENSE'
$testRoot = Join-Path $repoRoot '.superpowers\tmp-adb-tests'
$expected = @{
    'adb.exe'          = 'B4A6B455702684652CCCF7B46258B29E653538904359A58FD4931CF3EF286B3F'
    'AdbWinApi.dll'    = 'C1D653030B4BDE65D3E07E4D0B0979E17BE56DF1436CDD15528630F27808050D'
    'AdbWinUsbApi.dll' = '0710E894D9B40F71A670C13C694079D564C92C1279DA382CFE4850983AAEBE1B'
}

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

function Copy-Prepared([string]$Source, [string]$Name) {
    $target = Join-Path $testRoot $Name
    Copy-Item -LiteralPath $Source -Destination $target -Recurse
    return $target
}

function Replace-Ascii([string]$Path, [string]$Old, [string]$New) {
    $bytes = [IO.File]::ReadAllBytes($Path)
    $oldBytes = [Text.Encoding]::ASCII.GetBytes($Old)
    $newBytes = [Text.Encoding]::ASCII.GetBytes($New)
    if ($oldBytes.Length -ne $newBytes.Length) { throw 'Replacement strings must have equal length' }
    $offset = [Text.Encoding]::ASCII.GetString($bytes).IndexOf($Old, [StringComparison]::Ordinal)
    if ($offset -lt 0) { throw "String '$Old' not found in $Path" }
    [Array]::Copy($newBytes, 0, $bytes, $offset, $newBytes.Length)
    [IO.File]::WriteAllBytes($Path, $bytes)
}

Import-Module $validationModule -Force
Assert-AdbHost -WindowsHost $true -OperatingSystem64Bit $true -Architecture 'X64'
foreach ($hostTuple in @(
    @{ WindowsHost = $false; OperatingSystem64Bit = $true; Architecture = 'X64' },
    @{ WindowsHost = $true; OperatingSystem64Bit = $false; Architecture = 'X64' },
    @{ WindowsHost = $true; OperatingSystem64Bit = $true; Architecture = 'Arm64' }
)) {
    Assert-Fails 'requires a 64-bit x64 Windows host' { Assert-AdbHost @hostTuple }
}

$validVersion = @"
Android Debug Bridge version 1.0.41
Version 37.0.1-15733141
Installed as C:\adb.exe
Running on Windows
"@
Assert-AdbVersionOutput -Output $validVersion -ExitCode 0
foreach ($invalidVersion in @(
    $validVersion.Replace('Version 37.0.1-15733141', 'Version 37.0.1-157331410'),
    $validVersion.Replace('Version 37.0.1-15733141', 'prefix Version 37.0.1-15733141'),
    $validVersion.Replace('Version 37.0.1-15733141', 'version 37.0.1-15733141'),
    $validVersion.Replace('Android Debug Bridge version 1.0.41', 'Android Debug Bridge version 1.0.410')
)) {
    Assert-Fails 'ADB version mismatch' { Assert-AdbVersionOutput -Output $invalidVersion -ExitCode 0 }
}
Assert-Fails 'ADB version mismatch' { Assert-AdbVersionOutput -Output $validVersion -ExitCode 1 }
'PASS: rejected unsupported host tuples and inexact version lines'

if (-not (Test-Path -LiteralPath $prepare)) { throw "Production script is missing: $prepare" }
if (-not (Test-Path -LiteralPath $license)) { throw "Pinned license is missing: $license" }
if (Test-Path -LiteralPath $testRoot) { Remove-Item -LiteralPath $testRoot -Recurse -Force }
New-Item -ItemType Directory -Path $testRoot | Out-Null

try {
    $first = Join-Path $testRoot 'first'
    $second = Join-Path $testRoot 'second'
    $oldAndroidHome = $env:ANDROID_HOME
    $oldSdkRoot = $env:ANDROID_SDK_ROOT
    $env:ANDROID_HOME = Join-Path $testRoot 'not-installed'
    $env:ANDROID_SDK_ROOT = Join-Path $testRoot 'also-not-installed'
    try {
        & $prepare -ArchivePath $ArchivePath -Destination $first
        & $prepare -ArchivePath $ArchivePath -Destination $second
    } finally {
        $env:ANDROID_HOME = $oldAndroidHome
        $env:ANDROID_SDK_ROOT = $oldSdkRoot
    }

    $firstFiles = @(Get-ChildItem -LiteralPath $first -File | Sort-Object Name)
    if (($firstFiles.Name -join ',') -ne 'adb.exe,AdbWinApi.dll,AdbWinUsbApi.dll') {
        throw "Unexpected prepared file set: $($firstFiles.Name -join ',')"
    }
    foreach ($file in $firstFiles) {
        $hash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
        if ($hash -ne $expected[$file.Name]) { throw "Unexpected $($file.Name) hash: $hash" }
        $secondHash = (Get-FileHash -LiteralPath (Join-Path $second $file.Name) -Algorithm SHA256).Hash
        if ($secondHash -ne $hash) { throw "Clean preparations differ for $($file.Name)" }
    }
    & $prepare -VerifyOnly -Destination $first
    & (Join-Path $first 'adb.exe') version | Out-String | ForEach-Object {
        if ($_ -notmatch 'Version 37\.0\.1-15733141') { throw "Unexpected adb version: $_" }
    }

    $tamperedArchive = Join-Path $testRoot 'tampered.zip'
    Copy-Item -LiteralPath $ArchivePath -Destination $tamperedArchive
    $archiveBytes = [IO.File]::ReadAllBytes($tamperedArchive)
    $archiveBytes[$archiveBytes.Length - 1] = $archiveBytes[$archiveBytes.Length - 1] -bxor 1
    [IO.File]::WriteAllBytes($tamperedArchive, $archiveBytes)
    $failedDestination = Join-Path $testRoot 'failed-archive'
    Assert-Fails 'Archive SHA-256 mismatch' { & $prepare -ArchivePath $tamperedArchive -Destination $failedDestination }
    if (Test-Path -LiteralPath $failedDestination) { throw 'Failed archive preparation created packaging output' }

    $wrongName = Copy-Prepared $first 'wrong-name'
    Move-Item -LiteralPath (Join-Path $wrongName 'AdbWinApi.dll') -Destination (Join-Path $wrongName 'Wrong.dll')
    Assert-Fails 'Runtime file set mismatch' { & $prepare -VerifyOnly -Destination $wrongName }

    $extraFile = Copy-Prepared $first 'extra-file'
    [IO.File]::WriteAllText((Join-Path $extraFile 'unexpected.dll'), 'unexpected')
    Assert-Fails 'Runtime file set mismatch' { & $prepare -VerifyOnly -Destination $extraFile }

    $hiddenFile = Copy-Prepared $first 'hidden-file'
    $hiddenPath = Join-Path $hiddenFile 'hidden.txt'
    [IO.File]::WriteAllText($hiddenPath, 'hidden')
    [IO.File]::SetAttributes($hiddenPath, [IO.FileAttributes]::Hidden)
    Assert-Fails 'Runtime file set mismatch' { & $prepare -VerifyOnly -Destination $hiddenFile }

    $extraDirectory = Copy-Prepared $first 'extra-directory'
    New-Item -ItemType Directory -Path (Join-Path $extraDirectory 'hidden-directory') | Out-Null
    Assert-Fails 'Runtime file set mismatch' { & $prepare -VerifyOnly -Destination $extraDirectory }

    $wrongMachine = Copy-Prepared $first 'wrong-machine'
    $machinePath = Join-Path $wrongMachine 'adb.exe'
    $machineBytes = [IO.File]::ReadAllBytes($machinePath)
    $peOffset = [BitConverter]::ToInt32($machineBytes, 0x3c)
    $machineBytes[$peOffset + 4] = 0x64
    $machineBytes[$peOffset + 5] = 0x86
    [IO.File]::WriteAllBytes($machinePath, $machineBytes)
    Assert-Fails 'PE machine mismatch' { & $prepare -VerifyOnly -Destination $wrongMachine }

    $wrongVersion = Copy-Prepared $first 'wrong-version'
    Replace-Ascii (Join-Path $wrongVersion 'adb.exe') '37.0.1' '37.0.0'
    Assert-Fails 'SHA-256 mismatch' { & $prepare -VerifyOnly -Destination $wrongVersion }

    $wrongHash = Copy-Prepared $first 'wrong-hash'
    $hashPath = Join-Path $wrongHash 'AdbWinApi.dll'
    $hashBytes = [IO.File]::ReadAllBytes($hashPath)
    $hashBytes[$hashBytes.Length - 1] = $hashBytes[$hashBytes.Length - 1] -bxor 1
    [IO.File]::WriteAllBytes($hashPath, $hashBytes)
    Assert-Fails 'SHA-256 mismatch' { & $prepare -VerifyOnly -Destination $wrongHash }

    $wrongLicense = Join-Path $testRoot 'LICENSE'
    Copy-Item -LiteralPath $license -Destination $wrongLicense
    Add-Content -LiteralPath $wrongLicense -Value 'tampered'
    Assert-Fails 'License SHA-256 mismatch' { & $prepare -VerifyOnly -Destination $first -LicensePath $wrongLicense }

    'PASS: prepare-adb clean reproducibility and fail-closed verification'
} finally {
    if (Test-Path -LiteralPath $testRoot) { Remove-Item -LiteralPath $testRoot -Recurse -Force }
}
