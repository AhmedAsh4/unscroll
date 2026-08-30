# Unscroll V1 Design

Date: 2026-08-30  
Status: Approved in conversation; awaiting review of this written specification

## Summary

Unscroll is a fully free and open-source desktop application that helps people turn an ordinary Android phone into a quieter, deliberately limited device. A user connects a phone over USB, chooses the apps they want to keep, and applies a text-only launcher plus reversible Android restrictions. Blocked apps remain installed with their data intact, disappear from Unscroll Launcher, cannot launch normally, and stop sending notifications where the phone supports Android package suspension.

V1 prioritizes broad compatibility and meaningful friction rather than tamper-proof enforcement. A determined user can escape through ADB, Android recovery, factory reset, or manufacturer-specific controls. Normal reversal requires reconnecting the phone to any compatible Unscroll desktop installation and typing an explicit confirmation phrase.

## Goals

- Give everyday users a guided Windows desktop experience with no visible terminal, browser, Docker, or separately installed runtime.
- Show recognizable names and real installed icons for apps on the connected phone.
- Let the user choose an allowlist: selected apps stay available and all other safely suspendable launchable apps are blocked.
- Keep blocked apps and their data installed.
- Replace the current launcher with a text-only Unscroll Launcher based on Olauncher.
- Suppress normal launching and notifications for blocked apps.
- Restrict common app stores and sideload sources without root or factory-reset provisioning.
- Make every change recoverable from another compatible Unscroll installation.
- Fail safely when a device or manufacturer does not support a required operation.
- Work offline after installation and collect no telemetry.

## Non-goals for V1

- Root, custom ROM, Device Owner, kiosk, or parental-control enforcement.
- A guarantee against factory reset, recovery mode, another ADB client, or determined use of Android Settings.
- Multiple launcher choices.
- Work profiles, Private Space, Secure Folder, secondary Android users, or multi-device configuration.
- Wireless ADB.
- Usage analytics, screen-time statistics, schedules, streaks, gamification, cloud accounts, or accountability services.
- Native Linux, macOS, or Windows ARM64 packages. These follow the Windows x64 release in that order: Linux, then macOS.
- Reliably allowing automatic app updates while forbidding all new installs. V1 uses a deliberate maintenance flow instead.

## Platform and licensing

V1 supports Windows 10 and Windows 11 on x64. It ships as a normal signed installer that creates a standard Start-menu application. WSL is not supported or required.

All Unscroll-authored code is licensed under GPLv3. Unscroll Launcher is a clearly marked fork of GPLv3-licensed Olauncher, retaining upstream copyright and license notices. Bundled third-party code and tools retain their compatible FOSS licenses. Third-party notices remain available in the desktop application's About screen and repository.

The desktop stack is:

- Tauri 2 for the desktop shell and packaging.
- Rust for device discovery, ADB execution, planning, transactions, and recovery.
- Svelte for the desktop interface.
- Kotlin/Android for the Olauncher-based phone launcher.

V1 bundles a pinned ADB implementation so end users do not install platform tools separately. Redistribution of Google's binary distribution must pass a license review before release; if it does not, CI builds the required Windows ADB artifacts from the corresponding Apache-2.0 AOSP source. The result is checksummed and version-pinned in either case.

## System architecture

### Desktop application

The Tauri application contains four responsible areas:

1. **Device adapter:** starts the bundled ADB server, discovers devices, handles authorization state, and invokes commands without interpolating untrusted shell strings.
2. **Planner:** converts the device snapshot and user allowlist into a list of reversible operations. It refuses operations against protected or unknown packages.
3. **Transaction runner:** prepares recovery data, executes one operation at a time, verifies resulting state, journals progress on the phone, and resumes or rolls back after interruption.
4. **Svelte interface:** presents connection guidance, installed apps, review, progress, maintenance, editing, restoration, and actionable errors.

The device adapter is the only layer allowed to execute ADB. UI code cannot construct or run device commands directly.

### Unscroll Launcher

The Android application is a rebranded Olauncher fork with three Unscroll-specific responsibilities:

- Render only the current allowlist as a text-only launcher. The launcher itself does not display app icons.
- Build the primary-user app catalog and protected-package facts using Android platform APIs.
- Store versioned policy, recovery, and transaction data in private storage and expose the minimum required data through an ADB-shell-only bridge.

The fork removes Olauncher's on-phone hidden-app editor and any policy or restoration controls. It also removes features and permissions Unscroll does not need, including online wallpaper downloads, promotional links, usage statistics, Device Admin, and Accessibility Service integration. V1 keeps only the text home/app list, search, essential launcher behavior, and restrained appearance preferences. Policy writes remain available only through the ADB-shell bridge.

The bridge is a versioned Android `ContentProvider` protected for both reads and writes by `android.permission.DUMP`, a system permission held by the ADB shell. Normal third-party applications cannot use it. Preflight proves the bridge works on the connected device before any policy change. The provider exposes structured app metadata, icon streams, capability facts, active policy, recovery baseline, and transaction journal.

## Definitions and invariants

- **Allowed app:** a launchable primary-user app selected by the user or required for device safety.
- **Blocked app:** a launchable primary-user app Unscroll successfully suspended and omitted from the launcher.
- **Protected app:** a package Unscroll will never suspend because it owns a required system role or because safety cannot be established.
- **Recovery baseline:** the immutable record of relevant state before Unscroll first applies a policy.
- **Transaction journal:** the append-only record of changes Unscroll applies initially or during later edits and maintenance.

The following invariants always hold:

1. Unscroll never uninstalls a user's existing app or deletes its data.
2. Installing Unscroll Launcher without activating it is the only permitted bootstrap mutation before the baseline exists. If bridge or baseline preparation fails, Unscroll uninstalls that bootstrap copy. No existing package or setting is changed before a readable recovery baseline exists in launcher-private storage and at `/sdcard/Documents/Unscroll/recovery-v1.json`.
3. The baseline remains immutable until a complete restore succeeds.
4. Every mutation has a recorded inverse and is verified after execution.
5. The previous launcher remains installed and is recorded before Unscroll becomes the default launcher.
6. Unscroll Launcher becomes the default only after all other required operations succeed.
7. The recovery record is removed only after every recorded change has been restored and verified.
8. A package is never blocked unless Unscroll identifies it as launchable, proves it is not protected, and verifies the resulting suspended state.
9. Manifest contents are treated as untrusted input: device identity, schema, operation type, package identifiers, and setting values are validated before use. Manifests cannot contain arbitrary shell commands.

## Protected packages

Protection is role- and capability-based rather than a single manufacturer package-name list. Unscroll Launcher reports the current holders and dependencies for at least:

- System UI and Android Settings.
- Permission controller and package-management infrastructure.
- Active input method.
- Default dialer and emergency calling path.
- Default SMS app where telephony is present.
- Device provisioning and core account components when disabling them is unsafe.
- Unscroll Launcher and the currently active launcher until the final switch.
- Any package whose role or shared-package responsibilities cannot be classified safely.

Known manufacturer facts may add protection but never override dynamic safety checks. Conservative protection is preferable to disabling an uncertain system component. The review screen explains protected entries and reports packages that cannot safely be blocked.

## Primary user flow

### 1. Connect

Unscroll finds exactly one USB-connected Android device. It shows separate guidance for no device, unauthorized debugging, multiple devices, missing Windows/OEM drivers, unsupported Android versions, and unsupported ADB capabilities.

V1 supports Android 7 through Android 16 because the launcher base and package-suspension design target API 24 through API 36. Newer Android releases are shown as unverified until added to the compatibility suite. The interface states that support is best-effort across manufacturers.

### 2. Inspect

Unscroll installs its signed launcher APK without setting it as the default. It validates the ADB-shell bridge, then reads:

- Device model, Android/API version, manufacturer, serial/fingerprint binding, and current primary user.
- Current default launcher.
- Launchable apps with labels, real installed icons, package identifiers, and state.
- Protected package facts.
- Current package suspension and enabled states.
- Relevant app-store packages, install-intent handlers, unknown-source app-op values, and other settings Unscroll may modify.

If an active Unscroll policy already exists, the home screen offers Edit allowed apps, Store maintenance, or Restore phone instead of starting a new baseline.

### 3. Choose apps

The Guided Workspace layout uses a persistent left step rail: Connect, Choose apps, Review, and Apply. The main list shows each app's real installed icon, label, package identifier, and current keep/block state. Search and All/Kept/Blocked filters are available. Protected apps are selected and locked with a plain-language reason.

The selection model is allowlist-first: everything safely blockable is blocked unless the user keeps it. No policy is applied from this screen.

### 4. Review

The review screen separates:

- Apps that will remain available.
- Apps that will be suspended and hidden.
- Protected apps that cannot be selected.
- Stores and sideload sources Unscroll will restrict.
- Unsupported or manufacturer-protected items that may remain usable.

The Apply action remains disabled until required preflight checks pass.

### 5. Apply

Unscroll creates and verifies the recovery baseline, stores the intended policy, then applies operations in this order:

1. Configure the launcher's allowlist.
2. Suspend blocked apps using the supported package-manager shell command for that device.
3. Suspend detected user-facing app stores where safe.
4. Revoke and record supported per-source unknown-install app-ops without disabling shared permission-controller infrastructure.
5. Verify package, store, and setting state.
6. Set Unscroll Launcher as the default home activity.
7. Verify the default launcher and show completion results.

Each completed operation is added to the on-device journal. A required failure triggers inverse operations in reverse order. Unscroll refuses full setup if any detected user-facing app store cannot be suspended. Optional sideload restrictions that fail are reported as partial protection; Unscroll never labels a failed package or installation path as blocked.

### 6. Edit

Any compatible Unscroll desktop installation can read the active policy and baseline from the phone. Editing changes the allowlist and applies only the required delta. The original baseline is not replaced. Later Unscroll-created changes are appended to the journal so a full restore can undo them too.

### 7. Restore

Restore shows the recorded changes and requires the user to type `RESTORE MY PHONE`. It then:

1. Restores modified app-ops and settings.
2. Unsuspends packages Unscroll suspended, including later apps recorded by maintenance or editing.
3. Restores the previous default launcher.
4. Verifies restored state.
5. Removes Unscroll Launcher only after successful verification.
6. Removes the recovery records last.

If any step fails, the recovery records and launcher remain so the operation can be retried. Factory reset and manual ADB remain external escape hatches.

## Store maintenance

Ordinary ADB cannot reliably allow store updates while forbidding new installations. V1 therefore suspends detected stores during normal operation.

Store maintenance is available only with the phone connected to Unscroll. After a typed warning, Unscroll temporarily restores recorded store and install-source state. The user performs updates on the phone. When maintenance ends, Unscroll rescans installed packages, appends newly present launchable apps to the journal, suspends new apps that are not on the allowlist, reapplies store restrictions, and verifies the policy.

The interface states clearly that the maintenance window can also permit new installations. The desktop application prevents ordinary exit while maintenance is open and reapplies restrictions before closing. If the process, computer, or cable fails, the phone records maintenance as open; the next Unscroll connection closes it before offering any other action. Stores can remain usable between that failure and reconnection, which is an explicit limitation of broad ADB mode. The cable, desktop application, warning, rescan, and automatic re-blocking provide friction; they are not tamper-proof enforcement.

## Disconnection and recovery

The transaction runner treats the phone's actual state as authoritative. After reconnection it compares actual state with the baseline, intended plan, and journal. It offers only safe actions: resume the known plan, roll back verified completed operations, or export a diagnostic report. It does not guess when the records and device state cannot be reconciled.

The shared recovery copy allows a new desktop installation to recover after launcher data loss or accidental launcher removal. If both on-device copies are missing, Unscroll reports that automatic restoration cannot be proven safe and gives manual ADB/factory-reset guidance without executing speculative changes.

## Compatibility and honest claims

Unscroll runs capability probes for:

- Launcher APK installation and version compatibility.
- Shell access to the recovery bridge.
- Package suspension and unsuspension.
- Default-home selection.
- App-op inspection and modification.
- Store detection and safe restriction.

The core setup does not begin unless installation, bridge access, package suspension, home selection, recovery storage, and suspension of every detected user-facing store all pass. Per-source sideload restrictions may be partial and are shown as such.

V1 configures only the primary Android user. Work profiles, Private Space, Secure Folder, and secondary users are detected where possible and reported as outside the policy. Marketing language uses "most phones running Android 7–16" until real-device evidence justifies a broader claim.

## Security and privacy

- All functionality is local and offline after installation.
- There are no accounts, analytics, telemetry, advertisements, or remote APIs.
- Tauri capabilities expose only the commands required by the UI.
- ADB subprocess arguments use validated structured values; package identifiers never become arbitrary shell fragments.
- The recovery bridge is unavailable to ordinary phone apps.
- Recovery files use a versioned schema, device binding, strict operation allowlist, and corruption checksum.
- Release artifacts pin and checksum ADB and launcher resources.
- The launcher APK uses a stable release signing identity. Compatibility checks prevent an incompatible desktop release from overwriting it.
- Diagnostic export is explicit and previews included device/package metadata before saving.

## UI and accessibility

Unscroll uses a calm, accessible, functional visual system rather than a dashboard or wellness-themed marketing aesthetic:

- Warm neutral surfaces, dark readable text, and a restrained sage-green accent.
- Minimum WCAG AA contrast for normal text.
- Persistent progress and status for operations longer than 300 ms.
- Human-readable errors beside the failed step with a concrete recovery action.
- Visible labels, keyboard navigation, logical focus order, screen-reader announcements, and no color-only status.
- Reduced-motion support and no decorative continuous animation.
- Confirmation for policy application, maintenance, and restoration.
- No streaks, scores, guilt language, glassmorphism, excessive shadows, emoji icons, or digital-detox clichés.

The desktop app-selection list uses real icons extracted from the phone. Unscroll Launcher remains text-only by design.

## Windows packaging

V1 produces one consumer-facing Windows 10/11 x64 installer through Tauri's NSIS bundle. The installed app includes the compatible launcher APK and pinned ADB resources. It creates standard uninstall and Start-menu entries and does not require Node, Rust, Android SDK, or another runtime on the user's system.

Unscroll detects missing ADB connectivity and links to precise manufacturer-driver guidance. It does not silently install unsigned or manufacturer-specific kernel drivers.

Public release installers and executables are Authenticode-signed. As a FOSS project, Unscroll first pursues an open-source signing program such as SignPath Foundation. Unsigned development builds are labeled as such and are not presented as public releases.

## Testing strategy

### Automated tests

- Rust unit tests cover ADB output parsing, package validation, protected-package decisions, plan construction, inverse operations, journal reconciliation, and manifest validation.
- The transaction runner is tested against a fake ADB adapter for success, optional failure, required failure, disconnect, resume, rollback, and inconsistent-state cases.
- Svelte tests cover wizard state, filters, confirmations, keyboard navigation, focus, live announcements, and error recovery.
- Android tests cover allowlist filtering, protected-package facts, bridge permissions, baseline immutability, schema compatibility, journal updates, and icon streaming.
- Emulator integration covers representative supported API levels: Android 7, 10, 13, and Android 16/API 36.

### Real-device and release checks

Before a broad public V1 claim, smoke tests cover at least one Pixel-class device, one Samsung device, and one Xiaomi/Redmi-class device. The compatibility report records Android version, manufacturer, supported operations, and known limitations without collecting user data.

A clean Windows 10/11 x64 machine without development tools must pass this acceptance path:

1. Install and launch Unscroll from the normal installer.
2. Handle disconnected and unauthorized states with correct guidance.
3. Connect one supported phone and display real apps and icons.
4. Apply an allowlist without deleting app data.
5. Verify a blocked app is absent from Unscroll Launcher, cannot launch normally, and is suspended so its notifications are suppressed by Android.
6. Interrupt an apply operation and recover safely after reconnection.
7. Edit the allowlist from a separate compatible Unscroll installation.
8. Run store maintenance and re-block a newly installed unapproved app.
9. Restore the original state from the separate installation.
10. Confirm the flow remains usable by keyboard and a Windows screen reader.
11. Repeat the functional flow with networking disabled.

## Principal implementation risks

The implementation begins with narrow executable probes for these risks before building the full UI:

1. Shell package suspension and notification suppression across the targeted Android/OEM sample.
2. `DUMP`-protected bridge access and icon streaming across Android 7 through Android 16/API 36.
3. Default-launcher switching behavior across manufacturers.
4. Safe dynamic identification of protected packages and app stores.
5. ADB redistribution or reproducible AOSP build path for the Windows installer.

A failed core probe changes the compatibility claim or architecture before feature implementation. It is not hidden behind a fallback that weakens the promised behavior.

## Later directions

After the Windows V1 is stable, Unscroll can add Linux and then macOS packages, multiple supported launchers, multi-profile handling, and stronger Device Owner provisioning. A stronger update system should distinguish existing-app updates from new installations without an open maintenance window; that likely requires Android management privileges and is intentionally outside the broad-compatibility V1.
