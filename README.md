# Unscroll

Unscroll is a local, offline Windows companion for applying and restoring a reversible Android phone policy. This repository currently contains only the Task 1 bootstrap: an empty Tauri desktop application and a pinned Olauncher source snapshot.

The stable application identifiers are `org.unscroll.desktop` for the Windows desktop application and `org.unscroll.launcher` for the Android launcher.

## Development

Desktop dependencies are locked in `desktop/package-lock.json` and `desktop/src-tauri/Cargo.lock`. Android dependencies are locked in `launcher/settings-gradle.lockfile` and `launcher/app/gradle.lockfile`; use the included Gradle wrapper.

See [CONTRIBUTING.md](CONTRIBUTING.md) for local build commands and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for attribution.

## License

Unscroll-authored work is licensed under GPL-3.0-only. The imported Olauncher source remains GPL-3.0-only with its upstream notices preserved; see [launcher/UPSTREAM.md](launcher/UPSTREAM.md).

## Recovery

The applied phone policy persists without ADB and without the desktop app:
disabling Developer Options or unplugging the phone changes nothing on the
phone. To edit, maintain, or restore, re-enable USB debugging and reconnect.

Restoring asks for the typed phrase `RESTORE MY PHONE`, then rebuilds the
original state from the shared envelope at
`/sdcard/Documents/Unscroll/recovery-v1.json` and verifies it. Store
maintenance is deliberately frictional: opening the store shows a warning,
leaving is intercepted while maintenance is open, and closing rescans and
re-blocks anything unapproved.

Escape hatches, if Unscroll itself is unavailable: a factory reset always
returns the phone to stock, and manual `adb` commands can unsuspend packages
and reset the default launcher by hand. Everything works offline; there is
no telemetry, account, or upload in any path.

## Reinstall from another computer

Any compatible Unscroll install can take over: install it on another
Windows 10/11 x64 machine (see [docs/release/windows.md](docs/release/windows.md)),
connect the phone over USB, re-enable debugging, and reconcile. The new
machine keeps no private copy beforehand; it rebuilds everything from the
phone's shared envelope at `/sdcard/Documents/Unscroll/recovery-v1.json`.
