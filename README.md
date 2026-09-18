<div align="center">
  <img src="docs/images/logo.svg" alt="Unscroll logo" width="120"/>

# Unscroll — Turn an Android phone into a quieter device

**Pick the apps you want to keep. Unscroll hides and suspends the rest, swaps in a text-only launcher, and keeps everything reversible.**

[![License](https://img.shields.io/badge/License-GPL--3.0-blue.svg)](LICENSE)
[![Windows](https://img.shields.io/badge/Windows-10%20%2F%2011-blue.svg)](docs/release/windows.md)
[![Arch](https://img.shields.io/badge/arch-x64-blue.svg)](docs/release/windows.md)
[![CI](https://github.com/AhmedAsh4/unscroll/actions/workflows/ci.yml/badge.svg)](https://github.com/AhmedAsh4/unscroll/actions/workflows/ci.yml)

[Why Unscroll](#why-unscroll) • [How it works](#how-it-works) • [Quick start](#quick-start) • [Features](#key-features) • [FAQ](#frequently-asked-questions) • [Build from source](#build-from-source)

</div>

---

## Why Unscroll?

Most attempts at a quieter phone fail in familiar ways:

- Screen-time dashboards report the problem and change nothing
- App timers get dismissed with one tap
- Minimal launchers from the app store hide icons but leave every app, store, and notification one search away
- Serious lockdown needs root, a custom ROM, or a cloud account watching the device

**What Unscroll does instead:**

- A Windows PC applies the policy over USB — no root, no ROM, no account
- Blocked apps are suspended by the system, not just hidden: they leave the launcher, refuse to launch normally, and stop notifying where the phone supports it
- App stores and sideload sources are restricted, so new distractions cannot walk in the side door
- Nothing is uninstalled and no data is cleared — every change carries its own verified undo

---

## How it works

```
┌──────────────┐      USB + bundled ADB       ┌──────────────────────┐
│  Windows PC  │ ───────────────────────────▶ │    Android phone     │
│   Unscroll   │                              │                      │
│              │   1. reads installed apps    │  Unscroll Launcher   │
│  Connect →   │   2. records recovery        │  (text-only, your    │
│  Choose →    │      baseline                │   kept apps only)    │
│  Review →    │   3. suspends blocked apps   │                      │
│  Apply       │   4. restricts stores        │  Blocked apps: still │
│              │   5. sets default launcher   │  installed, data     │
└──────────────┘      (each step verified)    │  intact, suspended   │
                                              └──────────────────────┘
```

After setup the PC is out of the picture: the policy stays on the phone with USB debugging off. To edit, open store maintenance, or restore, you reconnect and re-enable debugging. A recovery record lives on the phone itself, so any compatible Unscroll install — including a fresh one on another computer — can take over or undo everything.

---

## Quick start

> The first installer release (v0.1.0) is on its way. Until it lands here, build from source below.

1. Download the Windows installer from the [Releases page](https://github.com/AhmedAsh4/unscroll/releases) and install it. No SDK, runtime, or driver package to hunt down — the installer carries its own ADB.
2. On the phone, enable Developer Options and USB debugging, then plug it in over USB.
3. Open Unscroll, check the connected phone, and choose the apps you want to keep. Protected system apps stay locked on.
4. Review what stays, what gets suspended, and which stores get restricted — then Apply.
5. When setup finishes, turn USB debugging back off. Done.

If Windows does not see the phone, Unscroll points at the right manufacturer driver page instead of installing anything silently. Full walkthrough: [docs/release/windows.md](docs/release/windows.md).

---

## Key features

| Feature | What it does |
| ------- | ------------ |
| Allowlist-first setup | Everything safely blockable is blocked unless you keep it; protected system apps stay locked with a plain reason |
| Text-only launcher | A fork of Olauncher that lists only your kept apps — no icons, no feeds, no on-phone settings to escape through |
| Verified suspension | Each blocked app is individually suspended and its state verified, not assumed from a command's exit code |
| Store restriction | Detected app stores are suspended (required — setup rolls back if a store cannot be blocked); sideload sources are restricted per source |
| Store maintenance | A deliberate, typed-warning window for updates that rescans afterward and re-blocks anything unapproved |
| Mirrored recovery | Every change is journaled to two on-phone copies before it runs and marked applied after verification; interruption resumes or rolls back, never guesses |
| Full restore | Rebuilds the recorded original state behind the typed phrase `RESTORE MY PHONE`, then removes Unscroll Launcher and its own data last |
| Offline and private | Works without network after installation; no accounts, analytics, or telemetry anywhere |

---

## Requirements

- **PC:** Windows 10 or 11, 64-bit x64
- **Phone:** Android 7 through 16 that passes Unscroll's on-device capability checks — the version number alone does not guarantee support, and some manufacturers need extra on-screen steps
- **Cable:** USB. Wireless setup is out of scope
- Only the primary Android user is covered; work profiles and secondary users are outside it

---

## Frequently asked questions

<details>
<summary><b>Does Unscroll delete my apps or their data?</b></summary>

No. Blocked apps stay installed with their data untouched. The only package Unscroll ever removes is its own launcher bootstrap copy after a failed setup — and only that copy.
</details>

<details>
<summary><b>Can someone just undo it on the phone?</b></summary>

Yes, and that is stated openly. ADB access, recovery mode, or a factory reset can undo the setup. Unscroll adds real friction — suspended apps, restricted stores, a launcher with nothing to tap — not tamper-proof enforcement.
</details>

<details>
<summary><b>Does it work offline? Does it track me?</b></summary>

After installation it works fully offline, and there is nothing to track you with: no accounts, no analytics, no telemetry, no network calls.
</details>

<details>
<summary><b>What if I lose access to the computer I set it up with?</b></summary>

Install Unscroll on any compatible Windows machine, reconnect the phone, and reconcile. The phone's shared recovery record (`/sdcard/Documents/Unscroll/recovery-v1.json`) carries everything needed to edit, maintain, or restore.
</details>

<details>
<summary><b>Why do stores stay blocked? Can apps still update?</b></summary>

Ordinary ADB cannot let updates through while forbidding new installs, so Unscroll suspends the stores during normal use. Store maintenance opens them temporarily for updates and re-blocks unapproved additions when it closes.
</details>

<details>
<summary><b>Which phones exactly are supported?</b></summary>

Phones running Android 7–16 (API 24–36) that pass capability probes for suspension, launcher switching, and recovery storage. Claims stay narrowed to tested hardware — see [docs/compatibility/device-matrix.md](docs/compatibility/device-matrix.md).
</details>

---

## Architecture

A Tauri 2 desktop app (Rust + Svelte) owns all device access, planning, and recovery. A Kotlin fork of Olauncher owns the text-only phone experience and an ADB-shell-only data bridge. A versioned recovery contract binds the two. Details live in [docs/superpowers/specs/2026-08-30-unscroll-v1-design.md](docs/superpowers/specs/2026-08-30-unscroll-v1-design.md); the build plan is beside it in `docs/superpowers/plans/`.

---

## Build from source

See [CONTRIBUTING.md](CONTRIBUTING.md) for commands. Dependencies are locked (`desktop/package-lock.json`, `desktop/src-tauri/Cargo.lock`, Gradle lockfiles) — use locked modes and the included Gradle wrapper. Stable application IDs: `org.unscroll.desktop` (Windows app) and `org.unscroll.launcher` (Android launcher).

---

## Contributing

Issues and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for setup and expectations.

---

## License

Unscroll-authored code is [GPL-3.0-only](LICENSE). The launcher is a fork of [Olauncher](https://github.com/tanujnotes/Olauncher) (GPL-3.0-only, upstream notices kept — see [launcher/UPSTREAM.md](launcher/UPSTREAM.md)). Bundled third-party tools keep their own licenses; the full list is in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
