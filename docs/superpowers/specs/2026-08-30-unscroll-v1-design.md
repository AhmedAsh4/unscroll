# Unscroll V1 Design

Date: 2026-08-30
Status: Revised after design review; awaiting final approval

## Summary

Unscroll is a fully free and open-source desktop application that helps people turn an ordinary Android phone into a quieter, deliberately limited device. A user connects a phone over USB, chooses the apps they want to keep, and applies a text-only launcher plus reversible Android restrictions. Blocked apps remain installed with their data intact, disappear from Unscroll Launcher, cannot launch normally, and stop sending notifications where the phone passes Unscroll's package-suspension capability checks.

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
- Store a versioned recovery envelope in private storage and expose the minimum required data through an ADB-shell-only bridge.

The fork removes Olauncher's on-phone hidden-app editor and any policy or restoration controls. It also removes features and permissions Unscroll does not need, including online wallpaper downloads, promotional links, usage statistics, Device Admin, and Accessibility Service integration. V1 keeps only the text home/app list, search, essential launcher behavior, and restrained appearance preferences. Policy writes remain available only through the ADB-shell bridge.

The bridge is a versioned Android `ContentProvider` protected for both reads and writes by `android.permission.DUMP`, a system permission held by the ADB shell. Normal third-party applications cannot use it. Preflight proves the bridge works on the connected device before any policy change. The provider exposes structured app metadata, icon streams, capability facts, active policy, recovery baseline, and transaction journal.

## Definitions and invariants

- **Allowed app:** a launchable primary-user app selected by the user or required for device safety.
- **Blocked app:** a launchable primary-user app Unscroll successfully suspended and omitted from the launcher.
- **Protected app:** a package Unscroll will never suspend because it owns a required system role or because safety cannot be established.
- **Recovery baseline:** the immutable record of relevant state before Unscroll first applies a policy.
- **Recovery envelope:** the baseline, active policy, maintenance state, and complete transaction journal required to reconcile or restore the phone.
- **Transaction journal:** the append-only record of changes Unscroll applies initially or during later edits and maintenance.

The following invariants always hold:

1. Unscroll never uninstalls a user's existing app or deletes its data.
2. Installing Unscroll Launcher without activating it is the only permitted bootstrap mutation before the baseline exists. If bridge or baseline preparation fails, Unscroll uninstalls that bootstrap copy. No existing package or setting is changed before a readable recovery envelope containing the baseline exists in launcher-private storage and at `/sdcard/Documents/Unscroll/recovery-v1.json`.
3. The baseline remains immutable until a complete restore succeeds.
4. Before every mutation, both envelope copies record the pending operation and its inverse. After verification, both copies record it as applied before the next mutation begins.
5. The baseline launcher remains installed and unsuspended for the entire policy lifetime, because it may provide recents, QuickStep, or gesture navigation. Unscroll records it before activation and hides it from Unscroll Launcher's app list.
6. Unscroll Launcher becomes the default only after all other required operations succeed.
7. The recovery envelope is removed only after every recorded change has been restored and verified.
8. A package is never blocked unless Unscroll identifies it as launchable, proves it is not protected, and verifies the resulting suspended state.
9. Manifest contents are treated as untrusted input: device identity, schema, operation type, package identifiers, and setting values are validated before use. Manifests cannot contain arbitrary shell commands.

## Recovery envelope

The private and shared copies mirror the complete recovery envelope after every pending or applied operation, not only the original baseline. Each envelope contains a baseline ID, a monotonic revision, the previous revision's hash, and a checksum. The desktop shell writes the shared copy at `/sdcard/Documents/Unscroll/recovery-v1.json`; the launcher bridge owns the private copy.

On reconnection, Unscroll validates both copies before trusting either. If one valid journal strictly extends the other, Unscroll uses it and repairs the stale copy. Forked histories, invalid checksums, mismatched baseline IDs, or device state that neither valid history explains stop automatic mutation; Unscroll never guesses.

If launcher-private data is cleared, Unscroll Launcher defaults to an empty policy view rather than exposing an inferred app list. A fresh compatible desktop installation can rebuild the private copy from the valid shared envelope before offering edit, maintenance, or restore actions.

## Protected packages

Protection is role- and capability-based rather than a single manufacturer package-name list. Unscroll Launcher reports the current holders and dependencies for at least:

- System UI and Android Settings.
- Permission controller and package-management infrastructure.
- Active input method.
- Default dialer and emergency calling path.
- Default SMS app where telephony is present.
- Device provisioning and core account components when disabling them is unsafe.
- Unscroll Launcher and the baseline launcher for the full policy lifetime.
- Any package whose role or shared-package responsibilities cannot be classified safely.

Known manufacturer facts may add protection but never override dynamic safety checks. Conservative protection is preferable to disabling an uncertain system component. The review screen explains protected entries and reports packages that cannot safely be blocked.

## Primary user flow

### 1. Connect

Unscroll finds exactly one USB-connected Android device. It shows separate guidance for no device, unauthorized debugging, multiple devices, missing Windows/OEM drivers, unsupported Android versions, and unsupported ADB capabilities.

V1 targets capability-tested phones running Android 7 through Android 16 because the required shell suspension and home-selection commands exist across API 24 through API 36. Newer Android releases are shown as unverified until added to the compatibility suite. Android version alone never establishes compatibility; the device must pass the required probes.

On MIUI and HyperOS, bootstrap guidance covers the `Install via USB` setting and actionable handling for `INSTALL_FAILED_USER_RESTRICTED` or `SecurityException` results. The interface warns that Xiaomi may require a Mi account, SIM, or network connection to enable the setting; Unscroll does not bypass those manufacturer requirements.

### 2. Inspect

Unscroll installs its signed launcher APK without setting it as the default. It validates the ADB-shell bridge, then reads:

- Device model, Android/API version, manufacturer, serial/fingerprint binding, and current primary user.
- Current default launcher.
- Launchable apps with labels, real installed icons, package identifiers, and state.
- Protected package facts.
- Current package suspension and enabled states.
- Relevant app-store packages, install-intent handlers, unknown-source app-op values, and other settings Unscroll may modify.

If an active Unscroll policy already exists, or launcher-private state can be recovered from the shared envelope, the home screen reconciles the two copies and offers Edit allowed apps, Store maintenance, or Restore phone instead of starting a new baseline.

### 3. Choose apps

The Guided Workspace layout uses a persistent left step rail: Connect, Choose apps, Review, and Apply. The main list shows each app's real installed icon, label, package identifier, and current keep/block state. Search and All/Kept/Blocked filters are available. Protected apps are selected and locked with a plain-language reason.

The selection model is allowlist-first: everything safely blockable is blocked unless the user keeps it. No policy is applied from this screen.

### 4. Review

The review screen separates:

- Apps that will remain available.
- Apps that will be suspended and hidden.
- Protected apps that cannot be selected, including the hidden baseline launcher.
- Stores and sideload sources Unscroll will restrict.
- Unsupported or manufacturer-protected items that may remain usable.

The Apply action remains disabled until required preflight checks pass.

### 5. Apply

Unscroll creates and verifies the recovery baseline, stores the intended policy, then applies operations in this order:

1. Configure the launcher's allowlist.
2. Suspend blocked apps individually using the supported package-manager shell command for that device, verifying each package's actual state.
3. Suspend detected user-facing app stores individually and verify each one.
4. Revoke and record supported per-source unknown-install app-ops without disabling shared permission-controller infrastructure.
5. Verify package, store, and setting state.
6. Ask Android's package manager to set Unscroll Launcher as the default home activity and verify the resolved HOME activity. If the manufacturer ignores the shell command, guide the user through Android's launcher chooser and verify again.
7. Verify the default launcher and show completion results.

Before each operation, Unscroll mirrors its pending journal entry and inverse to both recovery-envelope copies. After the resulting state is verified, it marks the operation applied in both copies before continuing. A required failure triggers inverse operations in reverse order.

If an ordinary selected-to-block app cannot be suspended and verified, Unscroll classifies it as `cannot fully block` and pauses before changing HOME. The paused result identifies the affected packages and offers only Continue with those apps available or Roll back. If any detected user-facing app store cannot be suspended and verified, setup is hard-gated and rolled back; V1 does not offer reduced store protection. Optional sideload restrictions that fail are reported as partial protection. Unscroll never labels a failed package or installation path as blocked.

After successful setup, Unscroll guides the user to disable USB debugging and, optionally, Developer Options. The policy persists without ADB. Editing, maintenance, and restoration require the user to re-enable debugging and reconnect the phone.

### 6. Edit

Any compatible Unscroll desktop installation can read and reconcile the recovery envelope from the phone, including restoring a missing private copy from the valid shared copy. Editing changes the allowlist and applies only the required delta. The original baseline is not replaced. Later Unscroll-created changes are appended to the mirrored journal so a full restore can undo them too.

### 7. Restore

Restore shows the recorded changes and requires the user to type `RESTORE MY PHONE`. It then:

1. Restores modified app-ops and settings.
2. Unsuspends packages Unscroll suspended, including later apps recorded by maintenance or editing.
3. Restores the previous default launcher through the shell command and verifies the resolved HOME activity; if the manufacturer ignores the command, guides the user through Android's launcher chooser and verifies again.
4. Verifies restored state.
5. Removes Unscroll Launcher and its private envelope copy only after successful verification.
6. Removes the shared recovery envelope last.

If restoration fails before cleanup, the recovery envelopes and launcher remain so the operation can be retried. If final cleanup alone fails, any remaining recovery data is retained for a safe retry. The flow explains how to re-enable USB debugging if it was disabled after setup. Factory reset and manual ADB remain external escape hatches.

## Store maintenance

Ordinary ADB cannot reliably allow store updates while forbidding new installations. V1 therefore suspends detected stores during normal operation.

Store maintenance is available only with the phone connected to Unscroll. After a typed warning, Unscroll temporarily restores recorded store and install-source state. The user performs updates on the phone. When maintenance ends, Unscroll rescans installed packages, appends newly present launchable apps to the journal, suspends new apps that are not on the allowlist, reapplies store restrictions, and verifies the policy.

The interface states clearly that the maintenance window can also permit new installations. The desktop application prevents ordinary exit while maintenance is open and reapplies restrictions before closing. If the process, computer, or cable fails, the phone records maintenance as open; the next Unscroll connection closes it before offering any other action. Stores can remain usable between that failure and reconnection, which is an explicit limitation of broad ADB mode. The cable, desktop application, warning, rescan, and automatic re-blocking provide friction; they are not tamper-proof enforcement.

## Disconnection and recovery

The transaction runner treats the phone's actual state as authoritative. After reconnection it validates and reconciles both envelope copies, then compares actual state with the baseline, intended plan, maintenance state, and complete journal. It offers only safe actions: resume the known plan, roll back verified completed operations, or export a diagnostic report. It does not guess when the records and device state cannot be reconciled.

The shared recovery copy allows a new desktop installation to recover after launcher data loss or accidental launcher removal. If both on-device copies are missing, Unscroll reports that automatic restoration cannot be proven safe and gives manual ADB/factory-reset guidance without executing speculative changes.

## Compatibility and honest claims

Unscroll runs capability probes for:

- Launcher APK installation and version compatibility.
- Shell access to the recovery bridge.
- Package suspension and unsuspension.
- Default-home selection through the shell command or a verified guided launcher-chooser fallback.
- App-op inspection and modification.
- Store detection and safe restriction.

The core setup does not begin unless installation, bridge access, shell support for package suspension, a viable home-selection path, recovery storage, and a safe suspension plan for every detected user-facing store all pass preflight. Actual per-package suspension is verified transactionally during Apply. Per-source sideload restrictions may be partial and are shown as such.

V1 configures only the primary Android user. Work profiles, Private Space, Secure Folder, and secondary users are detected where possible and reported as outside the policy. Marketing language uses "capability-tested Android 7-16 phones" and does not imply that Android version alone guarantees support.

## Security and privacy

- All functionality is local and offline after installation.
- There are no accounts, analytics, telemetry, advertisements, or remote APIs.
- Tauri capabilities expose only the commands required by the UI.
- ADB subprocess arguments use validated structured values; package identifiers never become arbitrary shell fragments.
- The recovery bridge is unavailable to ordinary phone apps.
- Recovery envelopes use a versioned schema, device binding, baseline ID, monotonic revision, previous-revision hash, strict operation allowlist, and corruption checksum.
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
- A post-setup security step recommends disabling USB debugging and Developer Options, with a universal warning that leaving debugging enabled can conflict with the security expectations of banking, government, workplace, or other sensitive apps.
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
- Emulator integration covers API 24, 28, 29, 33, and 36.

### Real-device and release checks

Before a broad public V1 claim, smoke tests cover at least one Pixel-class device, one Samsung device, and one Xiaomi/Redmi-class device. The compatibility report records Android version, manufacturer, supported operations, and known limitations without collecting user data.

A clean Windows 10/11 x64 machine without development tools must pass this acceptance path:

1. Install and launch Unscroll from the normal installer.
2. Handle disconnected and unauthorized states with correct guidance.
3. Connect one supported phone and display real apps and icons.
4. Apply an allowlist without deleting app data.
5. Verify a blocked app is absent from Unscroll Launcher, cannot launch normally, and is suspended so its notifications are suppressed by Android.
6. Verify the baseline launcher remains unsuspended and hidden while recents and gesture navigation continue to work.
7. Exercise both the shell HOME path and the guided launcher-chooser fallback.
8. Interrupt an apply operation and recover safely after reconnection.
9. Apply, edit, clear Unscroll Launcher data, and restore from a fresh desktop installation using the shared journal.
10. Run store maintenance and re-block a newly installed unapproved app.
11. Disable Developer Options, verify the policy persists, then re-enable debugging and restore the original state.
12. Verify MIUI/HyperOS bootstrap errors produce the documented actionable guidance.
13. Confirm the flow remains usable by keyboard and a Windows screen reader.
14. Repeat the functional flow with networking disabled.

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
