# Unscroll V1 on Windows (clean-machine guide)

Scope: Windows 10/11 x64 only. No Linux, macOS, ARM64, WSL, or Docker paths.
You need: the signed `Unscroll_<version>_x64-setup.exe` installer from a
published release, its `SHA256SUMS`, a USB cable, and one phone running
Android 7-16 (API 24-36). No development tools: no Node, Rust, Android SDK,
browser install, Docker, WSL, or PATH changes at any step.

## Install

1. Download the installer plus `SHA256SUMS` and `sbom.json` from the release.
2. Verify the checksum in PowerShell:
   `Get-FileHash .\Unscroll_<version>_x64-setup.exe -Algorithm SHA256`
   must match the `SHA256SUMS` line for the installer.
3. Confirm the Authenticode signature: right-click the file, Properties,
   Digital Signatures; or run
   `Get-AuthenticodeSignature .\Unscroll_<version>_x64-setup.exe`
   and require `Status: Valid`. An `*-unsigned-dev*` build is a developer
   artifact: never install it as a consumer release (see `signing.md`).
4. Run the installer. It creates a Start-menu entry (`Unscroll`) and an
   uninstall entry (Settings, Apps). It installs no drivers and changes no
   system services.

## Launch and operate

1. Launch Unscroll from the Start-menu entry. Install only a signed release
   installer: its filename must not carry the `*-unsigned-dev*` suffix that
   release.yml gives unsigned dev builds. That suffix is the whole
   unsigned-build label (an installer filename suffix; there is no in-app
   UI string), and it must be absent on a signed release build.
2. Connect one phone over USB with USB debugging on (Developer options).
   Wireless ADB is out of scope.
3. Follow the on-screen flow: inspect, choose the allowlist, review, apply.
   Kept apps keep their data; nothing is uninstalled or cleared.
4. The shared recovery envelope lives on the phone at
   `/sdcard/Documents/Unscroll/recovery-v1.json`. Copy it to safe storage
   if you want an extra backup; the phone copy is the source of truth.

## Restore and uninstall

- Restore the original state from any compatible Unscroll install by
  reconnecting the phone, re-enabling USB debugging, and reconciling from
  the shared envelope (see `README.md`, `## Recovery`). The typed phrase is
  `RESTORE MY PHONE`.
- Uninstall Unscroll from Settings, Apps. Uninstalling the desktop app does
  not change the phone: the applied policy persists until you restore it.
  To edit, maintain, or restore, re-enable USB debugging first.

## Offline repeat

After install, disable Windows networking and repeat
install, launch, inspect, apply, and restore. The full flow works offline:
no network fonts, fetches, accounts, analytics, or telemetry of any kind.

## USB drivers (links only)

If Windows does not show the phone over USB, install the manufacturer's
driver yourself, by hand, from the vendor's own pages:

- Google (Pixel/Nexus): [Get the Google USB Driver](https://developer.android.com/studio/run/win-usb)
  (OEM index: [Install OEM USB drivers](https://developer.android.com/studio/run/oem-usb)).
- Samsung: [Samsung Android USB Driver](https://developer.samsung.com/android-usb-driver)
  or [Samsung Smart Switch](https://www.samsung.com/us/support/owners/app/smart-switch)
  (its PC app can reinstall the device driver).
- Xiaomi/Redmi: start from the [OEM USB driver index](https://developer.android.com/studio/run/oem-usb)
  and take Mi Flash/drivers only from Xiaomi's official support channels.

Unscroll never installs, downloads, or silently bundles vendor drivers.
If a driver prompt appears, it comes from Windows or the vendor installer
you chose to run, never from Unscroll.
