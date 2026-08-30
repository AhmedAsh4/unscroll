# Unscroll

Unscroll is a local, offline Windows companion for applying and restoring a reversible Android phone policy. This repository currently contains only the Task 1 bootstrap: an empty Tauri desktop application and a pinned Olauncher source snapshot.

The stable application identifiers are `org.unscroll.desktop` for the Windows desktop application and `org.unscroll.launcher` for the Android launcher.

## Development

Desktop dependencies are locked in `desktop/package-lock.json` and `desktop/src-tauri/Cargo.lock`. Android dependencies are locked in `launcher/settings-gradle.lockfile` and `launcher/app/gradle.lockfile`; use the included Gradle wrapper.

See [CONTRIBUTING.md](CONTRIBUTING.md) for local build commands and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for attribution.

## License

Unscroll-authored work is licensed under GPL-3.0-only. The imported Olauncher source remains GPL-3.0-only with its upstream notices preserved; see [launcher/UPSTREAM.md](launcher/UPSTREAM.md).
