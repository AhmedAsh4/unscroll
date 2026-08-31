# Android Debug Bridge for Windows

Unscroll uses the three-file ADB runtime from Google's official Android SDK
Platform-Tools 37.0.1 Windows archive. The archive is downloaded during
preparation and is not committed.

```powershell
.\scripts\prepare-adb.ps1
.\scripts\prepare-adb.ps1 -VerifyOnly
```

For an offline preparation, download the immutable archive separately and run:

```powershell
.\scripts\prepare-adb.ps1 -ArchivePath C:\path\platform-tools_r37.0.1-win.zip
```

The script requires a 64-bit x64 Windows host, verifies the archive and retained
license notice, stages only `adb.exe`, `AdbWinApi.dll`, and `AdbWinUsbApi.dll`,
then verifies their exact names, SHA-256 values, PE machine fields, and reported
ADB version. Google's files are I386 PE (`0x014C`), not native AMD64; they run on
x64 Windows through Microsoft's built-in WOW64 compatibility layer.

Pins are recorded in [checksums.txt](checksums.txt). The complete upstream
notice and license material is retained in [LICENSE](LICENSE). The legal and
compatibility decision is documented in
[`docs/compatibility/adb-distribution-decision.md`](../../docs/compatibility/adb-distribution-decision.md).

The desktop/release owner owns upgrades. Changing the version requires the
primary-source, licensing, hash, architecture, two-clean-run, tamper, and
self-contained runtime evidence listed in the decision document before any pin
or packaged resource changes.
