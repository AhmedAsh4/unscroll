function Assert-AdbHost {
    param(
        [bool]$WindowsHost,
        [bool]$OperatingSystem64Bit,
        [string]$Architecture
    )

    if (-not $WindowsHost -or -not $OperatingSystem64Bit -or $Architecture -cne 'X64') {
        throw 'ADB preparation requires a 64-bit x64 Windows host with WOW64 support'
    }
}

function Assert-AdbVersionOutput {
    param(
        [AllowEmptyString()]
        [string]$Output,
        [int]$ExitCode
    )

    $multiline = [Text.RegularExpressions.RegexOptions]::Multiline
    $bridgeVersion = [regex]::Matches($Output, '^Android Debug Bridge version 1\.0\.41\r?$', $multiline).Count
    $platformVersion = [regex]::Matches($Output, '^Version 37\.0\.1-15733141\r?$', $multiline).Count
    if ($ExitCode -ne 0 -or $bridgeVersion -ne 1 -or $platformVersion -ne 1) {
        throw "ADB version mismatch: $($Output.Trim())"
    }
}

Export-ModuleMember -Function Assert-AdbHost, Assert-AdbVersionOutput
