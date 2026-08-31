[CmdletBinding()]
param(
    [string]$ArchivePath,
    [string]$Destination,
    [string]$LicensePath,
    [switch]$VerifyOnly
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path $PSScriptRoot -Parent
Import-Module (Join-Path $PSScriptRoot 'prepare-adb.validation.psm1') -Force
if (-not $Destination) { $Destination = Join-Path $repoRoot 'desktop\src-tauri\resources\adb' }
if (-not $LicensePath) { $LicensePath = Join-Path $repoRoot 'third_party\adb\LICENSE' }

$archiveUrl = 'https://dl.google.com/android/repository/platform-tools_r37.0.1-win.zip'
$archiveHash = '45F4D63113E895EBDE0C90F194099A4676B6AC653BD28D54314A9E022BBC1A99'
$licenseHash = '38EC8C6F5B7799C223FFEAB1F9E81C2D5FC67B5E56D6424F649630CA1EE1A811'
$sourcePropertiesHash = '2DCCD788C0234D8CF7F7457377E57F57527A86A629C6ED54FEB8AF0F549DAC38'
$runtime = [ordered]@{
    'adb.exe'          = @{ Hash = 'B4A6B455702684652CCCF7B46258B29E653538904359A58FD4931CF3EF286B3F'; Machine = 0x014c }
    'AdbWinApi.dll'    = @{ Hash = 'C1D653030B4BDE65D3E07E4D0B0979E17BE56DF1436CDD15528630F27808050D'; Machine = 0x014c }
    'AdbWinUsbApi.dll' = @{ Hash = '0710E894D9B40F71A670C13C694079D564C92C1279DA382CFE4850983AAEBE1B'; Machine = 0x014c }
}

function Assert-Hash([string]$Path, [string]$Expected, [string]$Label) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "$Label is missing: $Path" }
    $actual = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash
    if ($actual -ne $Expected) { throw "$Label SHA-256 mismatch: expected $Expected, got $actual" }
}

function Get-PeMachine([string]$Path) {
    $stream = [IO.File]::OpenRead($Path)
    try {
        $reader = [IO.BinaryReader]::new($stream)
        if ($reader.ReadUInt16() -ne 0x5a4d) { throw "Invalid PE DOS signature: $Path" }
        $stream.Position = 0x3c
        $peOffset = $reader.ReadInt32()
        if ($peOffset -lt 0 -or $peOffset + 6 -gt $stream.Length) { throw "Invalid PE offset: $Path" }
        $stream.Position = $peOffset
        if ($reader.ReadUInt32() -ne 0x00004550) { throw "Invalid PE signature: $Path" }
        return $reader.ReadUInt16()
    } finally {
        if ($reader) { $reader.Dispose() }
        $stream.Dispose()
    }
}

function Assert-Runtime([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Container)) { throw "Runtime directory is missing: $Path" }
    $children = @(Get-ChildItem -LiteralPath $Path -Force)
    $invalid = @($children | Where-Object {
        $_ -isnot [IO.FileInfo] -or
        ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
        $runtime.Keys -cnotcontains $_.Name
    })
    if ($children.Count -ne $runtime.Count -or $invalid.Count -ne 0) {
        throw "Runtime file set mismatch: expected $($runtime.Keys -join ', '); got $($children.Name -join ', ')"
    }

    foreach ($name in $runtime.Keys) {
        $pathToFile = Join-Path $Path $name
        $machine = Get-PeMachine $pathToFile
        if ($machine -ne $runtime[$name].Machine) {
            throw ('PE machine mismatch for {0}: expected 0x{1:X4}, got 0x{2:X4}' -f $name, $runtime[$name].Machine, $machine)
        }
        Assert-Hash $pathToFile $runtime[$name].Hash $name
    }

    $adb = Join-Path $Path 'adb.exe'
    $oldAndroidHome = $env:ANDROID_HOME
    $oldSdkRoot = $env:ANDROID_SDK_ROOT
    Remove-Item Env:ANDROID_HOME -ErrorAction SilentlyContinue
    Remove-Item Env:ANDROID_SDK_ROOT -ErrorAction SilentlyContinue
    try {
        $version = (& $adb version 2>&1 | Out-String)
        Assert-AdbVersionOutput -Output $version -ExitCode $LASTEXITCODE
    } finally {
        $env:ANDROID_HOME = $oldAndroidHome
        $env:ANDROID_SDK_ROOT = $oldSdkRoot
    }
}

Assert-AdbHost `
    -WindowsHost ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) `
    -OperatingSystem64Bit ([Environment]::Is64BitOperatingSystem) `
    -Architecture ([Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString())
Assert-Hash $LicensePath $licenseHash 'License'

if ($VerifyOnly) {
    if ($ArchivePath) { throw 'ArchivePath cannot be used with VerifyOnly' }
    Assert-Runtime $Destination
    "Verified ADB Platform-Tools 37.0.1 runtime at $Destination"
    exit 0
}

$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) ("unscroll-adb-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporaryRoot | Out-Null
try {
    if (-not $ArchivePath) {
        $ArchivePath = Join-Path $temporaryRoot 'platform-tools_r37.0.1-win.zip'
        Invoke-WebRequest -UseBasicParsing -Uri $archiveUrl -OutFile $ArchivePath
    }
    Assert-Hash $ArchivePath $archiveHash 'Archive'

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [IO.Compression.ZipFile]::OpenRead((Resolve-Path -LiteralPath $ArchivePath))
    try {
        $requiredEntries = @($runtime.Keys | ForEach-Object { "platform-tools/$_" }) +
            'platform-tools/NOTICE.txt', 'platform-tools/source.properties'
        foreach ($entryName in $requiredEntries) {
            if (@($archive.Entries | Where-Object FullName -CEQ $entryName).Count -ne 1) {
                throw "Archive filename mismatch: expected exactly one $entryName"
            }
        }

        $noticePath = Join-Path $temporaryRoot 'NOTICE.txt'
        $sourcePropertiesPath = Join-Path $temporaryRoot 'source.properties'
        foreach ($pair in @(
            @('platform-tools/NOTICE.txt', $noticePath),
            @('platform-tools/source.properties', $sourcePropertiesPath)
        )) {
            $entry = $archive.GetEntry($pair[0])
            $input = $entry.Open()
            $output = [IO.File]::Create($pair[1])
            try { $input.CopyTo($output) } finally { $output.Dispose(); $input.Dispose() }
        }
        Assert-Hash $noticePath $licenseHash 'Archive license'
        Assert-Hash $sourcePropertiesPath $sourcePropertiesHash 'Archive source.properties'

        $staged = Join-Path $temporaryRoot 'runtime'
        New-Item -ItemType Directory -Path $staged | Out-Null
        foreach ($name in $runtime.Keys) {
            $entry = $archive.GetEntry("platform-tools/$name")
            $input = $entry.Open()
            $output = [IO.File]::Create((Join-Path $staged $name))
            try { $input.CopyTo($output) } finally { $output.Dispose(); $input.Dispose() }
        }
    } finally {
        $archive.Dispose()
    }

    Assert-Runtime $staged
    if (Test-Path -LiteralPath $Destination) {
        $existing = @(Get-ChildItem -LiteralPath $Destination -Force)
        if ($existing.Count -ne 0 -and
            ($existing.Count -ne $runtime.Count -or @($existing | Where-Object { $runtime.Keys -cnotcontains $_.Name }).Count -ne 0)) {
            throw "Destination contains files outside the pinned runtime set: $Destination"
        }
    } else {
        New-Item -ItemType Directory -Path $Destination -Force | Out-Null
    }
    foreach ($name in $runtime.Keys) {
        Copy-Item -LiteralPath (Join-Path $staged $name) -Destination (Join-Path $Destination $name) -Force
    }
    Assert-Runtime $Destination
    "Prepared ADB Platform-Tools 37.0.1 runtime at $Destination"
} finally {
    if (Test-Path -LiteralPath $temporaryRoot) { Remove-Item -LiteralPath $temporaryRoot -Recurse -Force }
}
