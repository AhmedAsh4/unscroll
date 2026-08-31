# Unscroll V1 Windows Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. A fresh implementation agent owns each task, and the controller performs specification and quality review before the next task begins.

**Goal:** Deliver a GPLv3, consumer-ready Windows 10/11 x64 application that applies, edits, maintains, and safely restores an Unscroll policy on capability-tested Android 7-16 phones.

**Architecture:** A Tauri 2 desktop application owns all ADB access, planning, transactions, recovery, and the Svelte workflow. A Kotlin fork of Olauncher owns the text-only phone experience, primary-user catalog, protected-package facts, private recovery copy, and an ADB-shell-only provider. A versioned recovery contract and fixtures bind both applications and make interrupted operations recoverable from a fresh compatible desktop installation.

**Tech Stack:** Tauri 2, Rust stable, Svelte with TypeScript and Vite, Kotlin with the Android Gradle plugin, Android API 24-36, NSIS, GPLv3, and a pinned ADB distribution or reproducible AOSP build selected by the licensing gate.

**Spec:** `docs/superpowers/specs/2026-08-30-unscroll-v1-design.md`

## Global constraints

- Windows 10/11 x64 is the only packaged desktop target in V1. Linux, macOS, Windows ARM64, WSL, Docker, and wireless ADB remain out of scope.
- Android support is capability-tested API 24-36 for the primary user only. Android version alone is not a compatibility guarantee.
- Do not add root, Device Owner, kiosk, accessibility-service enforcement, work-profile handling, cloud services, telemetry, accounts, analytics, or multiple launchers.
- Existing user applications and their data are never uninstalled or cleared. Only the Unscroll bootstrap APK may be removed after failed bootstrap.
- The baseline launcher stays installed and unsuspended for the entire policy lifetime and is hidden from Unscroll Launcher.
- Every device mutation is structured, validated, reversible, journaled as pending to both recovery copies before execution, verified, and then marked applied in both copies.
- The shared recovery path is exactly `/sdcard/Documents/Unscroll/recovery-v1.json`.
- Store suspension is a hard gate. Failure to suspend any detected user-facing store rolls back setup.
- Ordinary app suspension failures pause before HOME changes and offer only continue with those apps available or rollback.
- HOME changes always require resolved-HOME verification and support a guided Android launcher-chooser fallback.
- The installed application works offline, exposes no terminal or browser UI, and requires no separately installed runtime.
- All Unscroll-authored code is GPLv3. Olauncher copyright and GPLv3 notices remain intact, and all bundled third-party artifacts retain notices and checksums.

## Approved UI contract

- The visual source of truth is `.superpowers/brainstorm/1610-1788083017/content/guided-workspace-icons-v2.html`. Before implementation in an isolated worktree, the controller must make this read-only artifact available to the UI agent.
- Tasks 16-19 require the `ui-ux-pro-max` skill for fidelity, interaction, and accessibility review. Its database recommendations may not replace or restyle the approved HTML direction.
- Implement only the app shell shown inside `.u-shell`. The brainstorm heading, subtitle, choice card, and inline selection handler outside that shell are not product UI.
- Preserve the mockup's warm neutral surface `#f6f3ec`, off-white cards `#fffdf8`, dark-green sidebar `#203b31`, primary sage-green `#245c47`, dark text family around `#183128`, muted text around `#5f6c65`, and light border family around `#cfd8d1`.
- Preserve the 54px top bar, 210px left step rail, four named steps, connected-device indicator, compact app table, 59px rows, 33px rounded app icons, count card, search, All/Kept/Blocked filters, status pills, and one clear primary action.
- Real icons come from the connected phone and appear only in the desktop chooser. Unscroll Launcher remains text-only. No online icon catalog or guessed brand art is allowed.
- Use Inter when available and the mockup's system sans-serif fallback stack without downloading fonts at runtime.
- Extend the same components and tokens to connection, review, progress, failure, maintenance, restore, and completion states. Do not introduce the rejected vibrant indigo style, dashboard cards, wellness gamification, glass effects, or decorative motion.
- Replace mockup-only spans with semantic controls while preserving appearance. Every action must work by keyboard, have visible focus, expose state to screen readers, avoid color-only meaning, announce dynamic progress, and respect reduced-motion settings.
- The desktop window targets the mockup's roughly 1050px content width, remains usable at 800px width, and must not introduce horizontal scrolling at the supported minimum size.

## Planned file ownership

- `contracts/` owns the recovery schema, bridge protocol, and valid and invalid cross-language fixtures.
- `desktop/src-tauri/src/adb/` owns process execution and ADB parsing; no other module may launch ADB.
- `desktop/src-tauri/src/device/` owns discovery, inspection, capability facts, and bootstrap state.
- `desktop/src-tauri/src/recovery/` owns envelope validation, hashing, mirroring, and reconciliation.
- `desktop/src-tauri/src/policy/` owns protection, store detection, operation planning, and inverses.
- `desktop/src-tauri/src/transaction/` owns apply, edit, maintenance, restore, resume, rollback, and state verification.
- `desktop/src-tauri/src/commands/` is the narrow Tauri boundary exposed to Svelte.
- `desktop/src/` owns presentation and user interaction only; it never constructs device commands.
- `launcher/app/src/main/java/org/unscroll/launcher/` owns launcher policy, catalog, protection facts, recovery storage, and bridge behavior.
- `docs/compatibility/` owns probe evidence and the tested-device matrix; `docs/release/` owns clean-machine acceptance and signing instructions.

## Agent execution protocol

Each task is an independent review gate. Its implementation agent must first add focused tests that fail for the missing behavior, confirm the intended failure, implement only the task scope, run the focused tests and all affected existing suites, inspect the diff for unrelated changes, and create the listed commit. Hardware-dependent checks must record device model, build fingerprint, API level, observed result, and limitations without collecting or uploading user data.

## Execution checklist

### Foundation and feasibility

- [x] Task 1: Bootstrap the licensed monorepo and pin Olauncher.
- [x] Task 2: Resolve and pin the Windows ADB distribution.
- [ ] Task 3: Define the recovery and bridge contracts.
- [ ] Task 4: Rebrand and reduce the Olauncher fork.
- [ ] Task 5: Add private recovery storage and policy-controlled launcher filtering.
- [ ] Task 6: Expose the protected ADB-shell bridge, catalog, roles, and icons.
- [ ] Task 7: Implement the sole Rust ADB adapter and device discovery.
- [ ] Task 8: Build and execute the compatibility risk gate.

### Policy engine

- [ ] Task 9: Inspect devices and perform mutation-free preflight.
- [ ] Task 10: Implement mirrored recovery storage and reconciliation.
- [ ] Task 11: Plan protected, reversible device operations.
- [ ] Task 12: Execute apply transactions with verification and rollback.
- [ ] Task 13: Reconnect, edit, restore, and export diagnostics.
- [ ] Task 14: Implement bounded store maintenance.
- [ ] Task 15: Expose a narrow Tauri API and desktop session state.

### Approved UI and release

- [ ] Task 16: Build the approved Guided Workspace shell and connection flow.
- [ ] Task 17: Build app selection and review with real phone icons.
- [ ] Task 18: Build apply, decision, HOME fallback, and completion UI.
- [ ] Task 19: Build active-policy, maintenance, restore, and diagnostics UI.
- [ ] Task 20: Complete automated integration and compatibility coverage.
- [ ] Task 21: Package, harden, sign, and accept the Windows release.

---

### Task 1: Bootstrap the licensed monorepo and pin Olauncher

**Depends on:** Approved V1 specification.

**Files:**

- Create root `LICENSE`, `README.md`, `CONTRIBUTING.md`, and `THIRD_PARTY_NOTICES.md`.
- Create the Tauri and Svelte workspace under `desktop/`, including `desktop/package.json`, `desktop/src/`, `desktop/src-tauri/Cargo.toml`, and `desktop/src-tauri/tauri.conf.json`.
- Import Olauncher under `launcher/` from upstream commit `952d9e942170a57583d0885a677e6f35c841b2ae`.
- Create `launcher/UPSTREAM.md` with the upstream repository, pinned commit, imported date, retained license, and future update procedure.
- Create `.github/workflows/ci.yml` for baseline desktop and Android builds.

**Work details:**

- Scaffold one Svelte/Vite frontend and one Tauri Rust library/application; do not add routing, state libraries, component kits, or CSS frameworks before a task requires them.
- Set stable application identifiers for the Windows desktop and Android launcher and document them once in the root README.
- Configure Android minimum API 24 and compile/target API 36 while preserving upstream notices and commit provenance.
- Keep the Olauncher import behaviorally unchanged in this task; reduction and rebranding belong to Task 4.
- Establish reproducible lockfiles and ignore only build products, generated packages, signing secrets, and local device reports.

**Verification:**

- A clean checkout resolves locked dependencies and builds the empty Tauri desktop application on Windows x64.
- The pinned Olauncher fork assembles and its existing unit test passes.
- CI runs both baseline builds and fails on license-file removal or lockfile drift.
- Repository search finds no telemetry SDK, cloud API, updater, or unapproved runtime dependency.

**Acceptance:** Both applications build from the pinned inputs, all license provenance is present, and no product behavior has been invented.

**Commit:** `chore: bootstrap Unscroll desktop and launcher`

### Task 2: Resolve and pin the Windows ADB distribution

**Depends on:** Task 1.

**Files:**

- Create `third_party/adb/README.md`, `third_party/adb/LICENSE`, and `third_party/adb/checksums.txt`.
- Create `scripts/prepare-adb.ps1` for the selected reproducible acquisition path.
- Reserve `desktop/src-tauri/resources/adb/` for the verified Windows x64 runtime files.
- Create `docs/compatibility/adb-distribution-decision.md`.

**Work details:**

- Complete the redistribution review before choosing a source. Prefer the official Google platform-tools bundle only if its redistribution terms permit public packaging; otherwise use a reproducible build from the corresponding Apache-2.0 AOSP source.
- Pin the exact version or source revision and SHA-256 for every shipped executable and DLL.
- Make preparation fail closed on checksum, filename, architecture, or license mismatch.
- Keep downloaded binaries out of ordinary source diffs unless their license and repository policy explicitly permit committing them.
- Document upgrade ownership and the exact release evidence required when changing the ADB version.

**Verification:**

- The preparation path produces the same checksums on two clean Windows runs.
- Tampering with one staged file causes verification to fail before packaging.
- The prepared ADB reports the pinned version and starts without a separately installed Android SDK.
- Third-party notices identify the selected source and license accurately.

**Acceptance:** The project has one legally reviewed, reproducible, checksummed ADB path suitable for the installer; unresolved redistribution questions block later packaging.

**Commit:** `build: pin Windows ADB resources`

### Task 3: Define the recovery and bridge contracts

**Depends on:** Task 1.

**Files:**

- Create `contracts/recovery-v1.schema.json` and `contracts/bridge-v1.md`.
- Create valid and invalid fixtures under `contracts/fixtures/recovery-v1/`.
- Create Rust contract models under `desktop/src-tauri/src/recovery/model.rs` and validation tests under `desktop/src-tauri/tests/recovery_contract.rs`.
- Create Kotlin contract models under `launcher/app/src/main/java/org/unscroll/launcher/recovery/RecoveryEnvelope.kt` and tests under `launcher/app/src/test/java/org/unscroll/launcher/recovery/RecoveryEnvelopeTest.kt`.

**Interfaces produced:**

- `RecoveryEnvelopeV1` containing schema version, device binding, baseline ID, monotonic revision, previous-revision hash, checksum, immutable baseline, active policy, maintenance state, and ordered journal.
- A closed operation vocabulary for launcher policy writes, package suspension changes, app-op changes, HOME changes, maintenance state, and cleanup. Every operation carries a typed inverse; no field can contain a shell command.
- Journal states for pending and applied, with stable operation IDs and enough recorded prior state to restore exactly what Unscroll changed.
- A versioned bridge protocol for device facts, catalog pages, icon streams, private-envelope reads and writes, and health checks.

**Work details:**

- Treat all JSON and provider arguments as hostile input. Reject unknown schema versions, unknown operation kinds, malformed package identifiers, invalid user IDs, invalid hashes, impossible revisions, duplicate operation IDs, oversized fields, and device-binding mismatches.
- Define deterministic canonical serialization for checksum and previous-revision hashing in both languages.
- Keep the baseline immutable after creation; edits change only active policy, maintenance state, revision chain, and journal.
- Include fixtures for a new baseline, pending mutation, applied mutation, maintenance-open state, strict extension, stale copy, checksum failure, forked history, baseline mismatch, and unknown operation.

**Verification:**

- Rust and Kotlin both accept every valid fixture and reject every invalid fixture with the expected classification.
- Round trips preserve canonical bytes and hashes across languages.
- The schema contains no arbitrary command or free-form executable field.
- Mutation tests prove baseline fields cannot change between revisions.

**Acceptance:** Both applications share one strict, versioned, corruption-detecting recovery contract before either begins mutating a phone.

**Commit:** `feat: define recovery and bridge contracts`

### Task 4: Rebrand and reduce the Olauncher fork

**Depends on:** Task 1.

**Files:**

- Modify `launcher/app/build.gradle`, `launcher/app/src/main/AndroidManifest.xml`, and Android resources under `launcher/app/src/main/res/`.
- Move the maintained source namespace from `app.olauncher` to `org.unscroll.launcher` while retaining upstream attribution in `launcher/UPSTREAM.md`.
- Modify the imported `MainActivity`, `MainViewModel`, app drawer, home, settings, preferences, and filtering files in their new namespace.
- Remove imported usage-statistics, wallpaper-download, accessibility-service, Device Admin, promotion, hidden-app editor, rename, and unnecessary settings files and resources.
- Add reduction tests under `launcher/app/src/test/java/org/unscroll/launcher/` and a manifest assertion in the Android test suite.

**Work details:**

- Rebrand visible application name, launcher icon, package namespace, and copy as Unscroll Launcher without erasing Olauncher authorship or license history.
- Retain only text home and app lists, search, launch behavior, shortcuts required for normal launcher use, and restrained local appearance preferences.
- Remove all policy editing, hiding, restoration, and escape controls from the phone UI.
- Remove permissions and components made unnecessary by deleted features, especially Usage Access, Accessibility Service, Device Admin, networking for wallpaper downloads, and promotional links.
- Do not add the Unscroll policy store or bridge in this task; produce a small, stable launcher base first.

**Verification:**

- The launcher installs and handles HOME and launcher intents on API 24 and API 36 emulators.
- The manifest contains no internet, usage-statistics, accessibility-service, or Device Admin capability.
- The phone UI contains no hidden-app editor, restore action, policy controls, online links, analytics, or promotional copy.
- Text list, search, app launching, back behavior, and rotation survive the reduction.

**Acceptance:** The fork is a minimal, text-only, correctly attributed Unscroll Launcher with no on-phone policy escape surface.

**Commit:** `feat: create minimal Unscroll Launcher fork`

### Task 5: Add private recovery storage and policy-controlled launcher filtering

**Depends on:** Tasks 3 and 4.

**Files:**

- Create `launcher/app/src/main/java/org/unscroll/launcher/recovery/PrivateEnvelopeStore.kt`.
- Create `launcher/app/src/main/java/org/unscroll/launcher/policy/ActivePolicyStore.kt` and `PolicyFilter.kt`.
- Modify the launcher view model and app-drawer adapter in `launcher/app/src/main/java/org/unscroll/launcher/` to consume policy-filtered entries.
- Add tests under `launcher/app/src/test/java/org/unscroll/launcher/policy/` and `launcher/app/src/test/java/org/unscroll/launcher/recovery/`.

**Interfaces consumed:** `RecoveryEnvelopeV1`, active allowlist, baseline launcher package, protected package facts, revision, and checksum rules from Task 3.

**Interfaces produced:** Atomic private-envelope reads and writes plus a launcher-visible list derived only from a valid active policy.

**Work details:**

- Persist the complete envelope atomically in app-private storage; a partial write must never replace the last valid copy.
- Do not infer a policy from installed apps. Missing, corrupt, or cleared private data produces an empty launcher policy view and a non-editable recovery-needed state.
- Show only allowed launchable packages after also removing Unscroll Launcher itself and the baseline launcher.
- Treat unknown packages, policy/schema mismatches, and stale revisions as unavailable rather than guessing.
- Keep the on-phone experience text-only and preserve search within the filtered list.

**Verification:**

- Tests cover valid allowlists, removed apps, newly installed apps, protected apps, hidden baseline launcher, invalid envelopes, missing envelopes, and launcher data clearing.
- An atomic-write interruption leaves the prior envelope readable.
- Clearing launcher data while Unscroll remains HOME yields an empty app list rather than an inferred unfiltered list.
- App icons are never rendered by the phone launcher.

**Acceptance:** Unscroll Launcher displays exactly the valid policy allowlist and fails closed when its private recovery state is unavailable.

**Commit:** `feat: enforce launcher allowlist policy`

### Task 6: Expose the protected ADB-shell bridge, catalog, roles, and icons

**Depends on:** Tasks 3 and 5.

**Files:**

- Create `launcher/app/src/main/java/org/unscroll/launcher/bridge/UnscrollProvider.kt` and protocol helpers beside it.
- Create `launcher/app/src/main/java/org/unscroll/launcher/catalog/AppCatalog.kt`, `ProtectedPackageResolver.kt`, `DeviceFacts.kt`, and `IconStream.kt`.
- Modify `launcher/app/src/main/AndroidManifest.xml` to register the provider with read and write protection through `android.permission.DUMP`.
- Add provider, catalog, protection, and icon tests under `launcher/app/src/test/java/org/unscroll/launcher/` and instrumentation tests under `launcher/app/src/androidTest/`.

**Interfaces consumed:** Bridge protocol and recovery contract from Task 3; private store from Task 5.

**Interfaces produced:** Versioned health, device-facts, app-catalog, icon-stream, private-envelope read, and private-envelope write operations available to ADB shell only.

**Work details:**

- Enumerate launchable applications for the primary user with label, package identifier, enabled and suspended state, icon metadata, and stable catalog ordering.
- Resolve protected holders dynamically for System UI, Settings, permission and package infrastructure, active input method, dialer and emergency path, SMS, provisioning/account dependencies, Unscroll Launcher, and the baseline launcher.
- Mark uncertain shared-role or manufacturer packages protected with a plain-language reason instead of relying on a universal package-name denylist.
- Stream real installed icons with bounded dimensions and payload size; never use the network or a brand catalog.
- Validate every provider argument and bound catalog and icon responses so a malformed shell call cannot exhaust memory or read arbitrary files.
- Reject normal application callers even when they know the provider authority.

**Verification:**

- Instrumentation proves ADB shell can use each intended operation and an ordinary test application cannot read or write the provider.
- Catalog tests cover duplicate labels, missing icons, adaptive icons, disabled apps, non-launchable packages, and stable paging or chunking.
- Role tests cover telephony and non-telephony devices, unresolved roles, and the permanent baseline-launcher protection rule.
- Icon streams decode correctly in a desktop-compatible fixture and stay within the defined bounds.

**Acceptance:** The desktop can obtain a safe primary-user catalog, real icons, capability facts, and private recovery access through one narrow shell-only bridge.

**Commit:** `feat: add launcher recovery bridge and catalog`

### Task 7: Implement the sole Rust ADB adapter and device discovery

**Depends on:** Tasks 1 and 2.

**Files:**

- Create `desktop/src-tauri/src/adb/process.rs`, `parser.rs`, `command.rs`, and `mod.rs`.
- Create `desktop/src-tauri/src/device/discovery.rs`, `identity.rs`, `errors.rs`, and `mod.rs`.
- Create representative ADB output fixtures under `desktop/src-tauri/tests/fixtures/adb/` and focused integration tests under `desktop/src-tauri/tests/`.

**Interfaces produced:** A single adapter that accepts closed, typed operations and returns structured device, command, stdout, stderr, exit, timeout, and transport results. A fake implementation is allowed because the transaction suite requires deterministic failure injection.

**Work details:**

- Launch only the bundled ADB executable with an argument array; never invoke a shell or interpolate a command string.
- Start and stop the bundled ADB server responsibly and scope every device command to the selected serial.
- Distinguish zero devices, exactly one device, multiple devices, unauthorized, offline, reconnecting, unsupported API, missing driver, timeout, and unexpected output.
- Validate serials, package identifiers, user IDs, components, app-op names, and file destinations before process execution.
- Classify MIUI and HyperOS bootstrap failures including `INSTALL_FAILED_USER_RESTRICTED` and `SecurityException` so the UI can provide manufacturer guidance.
- Redact sensitive values from diagnostic logs while preserving actionable command class and device facts.

**Verification:**

- Parser tests cover Windows line endings, daemon startup noise, unauthorized and offline states, multiple devices, timeouts, partial output, and non-zero exits.
- Injection tests prove package, component, serial, and path values cannot introduce additional arguments or shell syntax.
- The fake adapter can script success, ordinary failure, required failure, disconnect, reconnect, and inconsistent state without spawning a process.
- Repository search confirms this module is the only process-launching ADB owner.

**Acceptance:** All later device behavior routes through one typed, testable, injection-resistant adapter.

**Commit:** `feat: add safe ADB device adapter`

### Task 8: Build and execute the compatibility risk gate

**Depends on:** Tasks 2, 6, and 7.

**Files:**

- Create an internal probe entry point at `desktop/src-tauri/src/bin/unscroll_probe.rs` backed by the production ADB adapter.
- Create probe integration tests under `desktop/src-tauri/tests/device_probe.rs`.
- Create `docs/compatibility/probe-protocol.md` and `docs/compatibility/probe-results.md`.

**Work details:**

- Probe launcher installation and bridge access, catalog and icon streaming, package suspension and unsuspension, notification suppression observation, shell HOME selection, resolved-HOME verification, guided chooser availability, app-op inspection and restoration, store detection, and protected-package facts.
- Keep the probe executable development-only and explicitly exclude it from the NSIS resource and binary lists.
- Never probe by mutating an existing user app before a recovery baseline exists. Use a dedicated debug fixture package or an emulator snapshot whose entire state is disposable.
- Restore every probe mutation and verify restoration before reporting success.
- Run automated probe coverage on API 24 first and API 36 second, then record any available Pixel, Samsung, and Xiaomi or Redmi observations without claiming untested manufacturers.
- Stop feature work and revise the specification if shell suspension, bridge protection, recovery access, or both HOME paths cannot meet the promised behavior.

**Verification:**

- The probe produces machine-readable local results and a human-readable summary without telemetry or upload behavior.
- A forced failed restore leaves the fixture state and diagnostic evidence intact and returns a failing result.
- API 24 demonstrates the required suspension and HOME shell command paths rather than assuming a later Android minimum.
- Recorded evidence includes exact API, model or emulator image, build fingerprint, each capability outcome, and known limitation.

**Acceptance:** The five principal architecture risks have executable evidence before the full desktop workflow is built.

**Commit:** `test: add Android compatibility risk gate`

### Task 9: Inspect devices and perform mutation-free preflight

**Depends on:** Task 8 passing its architecture gate.

**Files:**

- Create `desktop/src-tauri/src/device/bootstrap.rs`, `inspect.rs`, `capabilities.rs`, and `catalog.rs`.
- Create `desktop/src-tauri/src/recovery/shared_copy.rs` for bounded reads and atomic writes at the fixed shared path.
- Add inspection and preflight fixtures and tests under `desktop/src-tauri/tests/`.

**Interfaces consumed:** Typed ADB adapter from Task 7 and bridge operations from Task 6.

**Interfaces produced:** `DeviceSnapshot`, `CapabilityReport`, `AppCatalogEntry`, `ProtectedPackageFact`, `StoreFact`, `InstallSourceFact`, and the private and shared recovery-copy observations used by later recovery logic.

**Work details:**

- Require exactly one authorized primary-user device, API 24-36, before inspection can succeed.
- Install the release-compatible launcher APK without activating it, verify signing and bridge protocol compatibility, and remove only that bootstrap installation if bridge or baseline preparation fails.
- Read model, manufacturer, API, fingerprint binding, serial, current user, resolved HOME, launchable apps and icons, protected facts, suspension state, enabled state, stores, install-intent handlers, unknown-source app-ops, and out-of-scope profiles.
- Validate shell support for suspension and unsuspension, bridge access, recovery storage, app-op inspection, and at least one viable HOME path without changing an existing application or setting.
- Detect an existing private or shared envelope before creating a new baseline. Return a recovery-required result rather than overwriting any existing evidence.
- Map Xiaomi restrictions to guidance for Install via USB, Mi account, SIM, and network prerequisites without trying to bypass them.

**Verification:**

- Tests cover zero, one, and multiple devices; unauthorized and offline devices; unsupported and newer-unverified APIs; bridge mismatch; launcher signature mismatch; missing shared storage; secondary profiles; and MIUI bootstrap errors.
- App icons survive the bridge and Rust boundary with correct package association and bounded size.
- No preflight test records an existing package, app-op, or HOME mutation before a valid baseline exists.
- Bootstrap cleanup removes only the just-installed Unscroll Launcher when safe and never removes a pre-existing compatible launcher installation.

**Acceptance:** The desktop can produce a complete, trustworthy snapshot or a precise no-mutation failure result for one connected phone.

**Commit:** `feat: inspect Android device capabilities`

### Task 10: Implement mirrored recovery storage and reconciliation

**Depends on:** Tasks 3, 7, and 9.

**Files:**

- Create `desktop/src-tauri/src/recovery/checksum.rs`, `validate.rs`, `mirror.rs`, `reconcile.rs`, and `mod.rs`.
- Extend `desktop/src-tauri/src/recovery/shared_copy.rs` and contract fixtures.
- Add `desktop/src-tauri/tests/recovery_reconciliation.rs`.

**Interfaces produced:** Recovery initialization, pending-revision append, applied-revision append, stale-copy repair, private-copy rebuild, cleanup eligibility, and reconciliation outcomes limited to consistent, strict extension, fork, corruption, baseline mismatch, missing copies, and unexplained device state.

**Work details:**

- Create the immutable baseline from the inspected device snapshot and write revision zero to launcher-private storage and the fixed shared path before any user-state mutation.
- Write shared data through a temporary file and verified replacement; use the bridge's atomic private write for the other copy.
- Represent every change to envelope state as a new monotonic revision linked to the previous canonical hash. A crash between the two writes may leave one valid strict extension, never two unrelated histories.
- Before a device mutation, persist the pending operation and inverse to both copies and read them back. After state verification, persist the applied result to both copies before permitting the next mutation.
- Select and repair only when one valid chain strictly extends the other. Stop on forks, checksum failure, baseline mismatch, revision gaps, unknown operations, or device state unexplained by either chain.
- Rebuild a missing private copy from a valid shared envelope, including after launcher data clearing or reinstallation by a fresh compatible desktop.
- Permit envelope removal only after restoration verification; remove the private copy with launcher cleanup and the shared copy last.

**Verification:**

- Tests interrupt every boundary before and after each of the two writes and prove deterministic reconciliation.
- Tests cover stale private, stale shared, missing private, missing shared, both missing, corrupted newest copy, forked histories, baseline mismatch, unexplained device state, and cleanup retry.
- A recovery baseline mutation is rejected even when its resulting checksum is otherwise valid.
- Reconciliation never silently chooses by timestamp, file size, or device state guesswork.

**Acceptance:** Any interrupted envelope update is either safely repairable by strict history or explicitly blocked without speculative device changes.

**Commit:** `feat: add mirrored recovery reconciliation`

### Task 11: Plan protected, reversible device operations

**Depends on:** Tasks 9 and 10.

**Files:**

- Create `desktop/src-tauri/src/policy/operation.rs`, `protection.rs`, `stores.rs`, `planner.rs`, and `mod.rs`.
- Add planner tests under `desktop/src-tauri/tests/policy_planner.rs` with device-snapshot fixtures.

**Interfaces produced:** Closed plans for initial apply, allowlist edit, maintenance open and close, rollback, and full restore. Each operation includes preconditions, expected post-state, typed inverse, requirement level, and user-visible description.

**Work details:**

- Build allowlist-first plans only from packages present in the inspected primary-user catalog.
- Permanently exclude System UI, Settings, permission and package infrastructure, active input method, emergency and telephony roles, uncertain shared-role packages, Unscroll Launcher, and the baseline launcher from suspension plans.
- Hide but never suspend the baseline launcher, even after Unscroll becomes HOME.
- Detect stores from user-facing launchability, install intent handling, current roles, and reviewed manufacturer facts. Do not let a static name list override dynamic safety protection.
- Plan supported per-source unknown-install app-op changes without disabling shared permission infrastructure.
- Classify store operations as required, unknown-source app-op operations as optional, and ordinary app suspension as user-resolvable on verified failure.
- Preserve exact prior values in inverses and refuse plans containing an unknown package, user, component, app-op, or operation kind.

**Verification:**

- Tests prove protected packages and the baseline launcher can never enter a suspension operation, even when selected explicitly or mislabeled by catalog input.
- Tests cover no telephony, multiple stores, manufacturer store, shared permission controller, unsupported app-op, newly installed app, removed app, and unchanged edit delta.
- Initial apply always orders policy write, ordinary app suspension, store suspension, app-ops, aggregate verification, and HOME last.
- Restore is the exact reverse of recorded Unscroll changes, not a reset to assumed Android defaults.

**Acceptance:** Every allowed V1 workflow becomes a finite, reviewable plan with safe protection and complete inverses; arbitrary operations are unrepresentable.

**Commit:** `feat: plan reversible Unscroll policies`

### Task 12: Execute apply transactions with verification and rollback

**Depends on:** Tasks 10 and 11.

**Files:**

- Create `desktop/src-tauri/src/transaction/runner.rs`, `journal.rs`, `verify.rs`, `outcome.rs`, and `mod.rs`.
- Create `desktop/src-tauri/tests/apply_transaction.rs` using the fake ADB adapter.

**Interfaces produced:** Transaction events for progress, verified completion, ordinary-app decision required, guided launcher chooser required, optional protection warning, recoverable disconnect, rollback progress, rollback failure, and inconsistent state.

**Work details:**

- Execute one planned operation at a time only after mirrored pending persistence and stop before the next operation until actual state is verified and mirrored as applied.
- Verify package suspension per package rather than trusting command exit alone.
- On ordinary app failure, record the package as cannot fully block and pause before HOME. Accept only continue with that package available or rollback; update active policy and journal consistently for either choice.
- On any detected store failure, automatically roll back all completed changes in reverse order and never offer reduced store protection.
- Record optional sideload restriction failures as partial protection without claiming the path is blocked.
- Change HOME last. Try the shell command, resolve HOME, and emit a guided chooser state if the manufacturer ignored it; continue only after a second resolved-HOME verification.
- On disconnect or process termination, leave the mirrored journal sufficient for later resume or rollback. Never run speculative cleanup.

**Verification:**

- Fake-adapter tests cover full success, each operation failing, false-success command output, ordinary continue, ordinary rollback, store hard failure, optional app-op failure, shell HOME success, chooser fallback, chooser cancellation, disconnect at every boundary, and rollback failure.
- Every test asserts journal revisions, both envelope copies, actual fake-device state, baseline immutability, and HOME ordering.
- A successful apply leaves blocked apps and stores suspended, Unscroll resolved as HOME, baseline launcher unsuspended, and partial restrictions honestly classified.
- A completed rollback restores only recorded changes and retains recovery evidence if any inverse cannot be verified.

**Acceptance:** Initial setup is transactional, per-operation verified, interruption-safe, and honest about partial or failed protection.

**Commit:** `feat: apply Unscroll policy transactionally`

### Task 13: Reconnect, edit, restore, and export diagnostics

**Depends on:** Task 12.

**Files:**

- Create `desktop/src-tauri/src/transaction/session.rs`, `edit.rs`, `restore.rs`, and `diagnostics.rs`.
- Add `desktop/src-tauri/tests/session_recovery.rs`, `edit_transaction.rs`, and `restore_transaction.rs`.

**Interfaces produced:** Session classification for new setup, active policy, maintenance recovery, resumable transaction, rollback-only transaction, restore-ready, cleanup retry, and blocked inconsistency; edit and restore event streams; explicit diagnostic preview and export data.

**Work details:**

- Reconcile both envelope copies before offering any action and compare the valid history with actual package, app-op, HOME, and maintenance state.
- Offer only resume, rollback, restore, or diagnostic export when those actions are proven safe by the journal.
- Compute edits as a delta against the active policy while preserving the original baseline and appending every new change to the same mirrored history.
- Require exact typed confirmation `RESTORE MY PHONE` before restore begins.
- Restore app-ops and settings, unsuspend only packages Unscroll recorded as suspended, restore and verify baseline HOME through shell or guided chooser, verify the full restored state, remove Unscroll Launcher and private data, and delete the shared envelope last.
- If only final cleanup fails, retain remaining recovery data and expose cleanup retry without repeating restored mutations.
- Preview all model, fingerprint, package, operation, and error metadata before a user chooses a local diagnostic export destination; never upload it.

**Verification:**

- Apply, edit, clear launcher data, reconnect from a fresh desktop installation, rebuild private state from shared recovery, and restore successfully in the fake integration suite.
- Tests cover stale-copy repair, safe resume, safe rollback, unexplained state block, typed-confirmation mismatch, HOME chooser fallback, launcher uninstall failure, shared cleanup failure, and diagnostic redaction.
- Restore never unsuspends a package that was already suspended before the baseline and never replaces the baseline during edit.
- A missing pair of recovery copies yields manual ADB and factory-reset guidance only, with no automated mutations.

**Acceptance:** Any compatible Unscroll installation can safely manage or restore a valid policy, while ambiguous state stops without guessing.

**Commit:** `feat: recover edit and restore Unscroll sessions`

### Task 14: Implement bounded store maintenance

**Depends on:** Task 13.

**Files:**

- Create `desktop/src-tauri/src/transaction/maintenance.rs`.
- Add `desktop/src-tauri/tests/store_maintenance.rs`.
- Extend session classification and planner files only where maintenance uses their existing contracts.

**Interfaces produced:** Maintenance warning accepted, maintenance open, awaiting user updates, closing, recovered-after-disconnect, verified closed, and close-failed states.

**Work details:**

- Require a connected phone and typed warning acknowledgment before opening maintenance.
- Journal maintenance open before temporarily restoring recorded store and install-source state.
- While open, expose a state that prevents ordinary desktop exit and clearly states that new installations are possible.
- On close, rescan installed packages, append newly present launchable packages to the journal, preserve allowed additions, suspend unapproved additions, reapply store and install-source restrictions, and verify the active policy.
- When a session reconnects with maintenance marked open, close it before offering edit, restore, or another maintenance window.
- Do not attempt to distinguish store updates from new installs or promise automatic updates outside this explicit window.

**Verification:**

- Tests cover normal open and close, no installed changes, approved update, newly installed allowed app, newly installed blocked app, new store, disconnect after every operation, process termination, close failure, and next-session forced close.
- Stores remain hard-gated during close; failure retains recovery data and a maintenance-open state.
- Newly discovered packages are journaled before suspension so full restore can undo only Unscroll changes.
- No exit path silently abandons a connected maintenance session without attempting verified close.

**Acceptance:** Maintenance provides deliberate update friction, records its temporary exposure, and reliably re-establishes the policy after reconnection.

**Commit:** `feat: add guarded store maintenance`

### Task 15: Expose a narrow Tauri API and desktop session state

**Depends on:** Tasks 9-14.

**Files:**

- Create `desktop/src-tauri/src/app_state.rs`.
- Create `desktop/src-tauri/src/commands/device.rs`, `policy.rs`, `maintenance.rs`, `recovery.rs`, `diagnostics.rs`, and `mod.rs`.
- Modify `desktop/src-tauri/src/lib.rs` and the minimal Tauri capability files under `desktop/src-tauri/capabilities/`.
- Create frontend transport and DTO files at `desktop/src/lib/api/invoke.ts`, `events.ts`, and `types.ts`.
- Add command-boundary tests under `desktop/src-tauri/tests/tauri_commands.rs` and TypeScript contract fixtures under `desktop/src/lib/api/`.

**Interfaces produced:** Frontend operations for device discovery and inspection, session reconciliation, apply start and decision response, edit start, maintenance open and close, restore start, diagnostic preview and export, plus typed progress events. No interface accepts raw ADB arguments or a caller-authored operation plan.

**Work details:**

- Keep Tauri commands thin: validate UI input, call the responsible Rust service, map domain outcomes to stable DTOs, and emit progress.
- Bind every active operation to the inspected device serial and fingerprint; reject device replacement during a session.
- Permit only one mutating transaction at a time and define safe cancellation as a domain request handled by the transaction runner, never abrupt task abandonment.
- Expose app icons through bounded Tauri resource data and release memory when the catalog changes.
- Limit filesystem access to explicit diagnostic export destinations and packaged resources. Do not expose general shell, process, or filesystem capabilities to Svelte.
- Keep error categories stable and user-actionable without leaking raw commands or unredacted device data.

**Verification:**

- Boundary tests reject malformed package IDs, stale device IDs, concurrent mutations, raw command-shaped payloads, invalid confirmations, and export paths without user selection.
- Progress event ordering matches journal ordering for success, pause, resume, rollback, disconnect, maintenance, and restore.
- Capability inspection proves the webview cannot start a process, issue ADB, or read arbitrary local files.
- Rust and TypeScript DTO fixtures agree on all states and enum values.

**Acceptance:** Svelte receives everything needed for the guided workflow through a small typed boundary while all authority remains in Rust.

**Commit:** `feat: expose Unscroll desktop commands`

### Task 16: Build the approved Guided Workspace shell and connection flow

**Depends on:** Tasks 15 and the approved HTML artifact.

**Files:**

- Create a tracked reference at `docs/design/unscroll-v1-guided-workspace.html` from `.superpowers/brainstorm/1610-1788083017/content/guided-workspace-icons-v2.html`, preserving the app shell and removing only the brainstorm wrapper and encoding artifacts.
- Create `desktop/src/styles/tokens.css` and `desktop/src/styles/global.css`.
- Create `desktop/src/lib/components/AppShell.svelte`, `TitleBar.svelte`, `StepRail.svelte`, `DeviceCard.svelte`, `ConnectionStatus.svelte`, and `InlineNotice.svelte`.
- Create `desktop/src/lib/screens/ConnectScreen.svelte` and `desktop/src/lib/state/workspace.ts`.
- Modify `desktop/src/App.svelte` and add focused component tests beside the Svelte files or under `desktop/src/tests/`.

**Interfaces consumed:** Device discovery, inspection, reconciliation, and stable error DTOs from Task 15.

**Work details:**

- Reproduce the approved HTML shell and tokens exactly enough that side-by-side review finds no material change in hierarchy, density, color, shape, or brand treatment.
- Use native semantic buttons, headings, status regions, and navigation landmarks while keeping the visual 54px title bar and 210px step rail.
- Implement Connect, Choose apps, Review, and Apply step states with complete, active, pending, and blocked semantics.
- Cover no device, connected, unauthorized, offline, multiple devices, missing driver, unsupported API, unverified newer API, incompatible launcher, recovery found, maintenance found, and MIUI or HyperOS bootstrap guidance.
- Keep operation feedback persistent after 300ms and announce connection and inspection changes through polite live regions.
- Use system font fallbacks and local assets only; no network fonts, external icon fetches, dashboard library, component kit, or CSS framework.

**Verification:**

- Visual comparison at approximately 1050px matches the approved HTML shell, and the layout remains usable at 800px without horizontal scrolling.
- Keyboard order follows title bar, step rail context, main content, and primary action; every focus state is visible.
- Screen-reader tests announce device and error changes once, with the recovery action adjacent to the error.
- Reduced-motion tests remove nonessential transitions, and high-contrast checks meet WCAG AA for normal text.

**Acceptance:** The real desktop application uses the approved Guided Workspace design and guides every connection state without exposing technical command details.

**Commit:** `feat: build Guided Workspace connection flow`

### Task 17: Build app selection and review with real phone icons

**Depends on:** Tasks 15 and 16.

**Files:**

- Create `desktop/src/lib/screens/ChooseAppsScreen.svelte` and `ReviewScreen.svelte`.
- Create `desktop/src/lib/components/AppToolbar.svelte`, `AppList.svelte`, `AppRow.svelte`, `StatePill.svelte`, `SelectionSwitch.svelte`, `CountCard.svelte`, and `ReviewGroup.svelte`.
- Extend `desktop/src/lib/state/workspace.ts` and add focused Svelte tests.

**Interfaces consumed:** Inspected catalog entries, real icon data, protected reasons, store facts, install-source facts, and capability report from Task 15.

**Work details:**

- Reproduce the approved chooser: real 33px installed icon, label, package identifier, state pill, and switch in each compact row.
- Implement case-insensitive search and All, Kept, and Blocked filters without changing the underlying selection.
- Default to allowlist-first behavior. Protected entries stay selected and locked with a visible plain-language reason; the baseline launcher is explained on desktop but omitted from the phone launcher.
- Keep the selected count synchronized and expose the result through text, not color alone.
- Group review into kept, suspended and hidden, protected, stores and sideload sources, and unsupported or manufacturer-protected items.
- Disable Apply until preflight is valid. Moving between Choose and Review must never mutate the phone.
- Handle missing or failed icon streams with a neutral local fallback carrying the app label; never substitute guessed brand art.

**Verification:**

- Tests cover search, each filter, toggling, protected locks, duplicate labels, missing icons, large catalogs, count updates, back navigation, and no-device invalidation.
- Keyboard and screen-reader tests operate every switch and announce label, package, current state, and protected reason.
- A backend spy proves Choose and Review issue no mutating command.
- Visual comparison preserves the HTML mockup's icon, row, toolbar, count, state-pill, and primary-action treatment.

**Acceptance:** Everyday users can confidently choose an allowlist from recognizable installed apps and understand the complete effect before mutation.

**Commit:** `feat: add app selection and policy review`

### Task 18: Build apply, decision, HOME fallback, and completion UI

**Depends on:** Tasks 12, 15, 16, and 17.

**Files:**

- Create `desktop/src/lib/screens/ApplyScreen.svelte` and `CompletionScreen.svelte`.
- Create `desktop/src/lib/components/OperationProgress.svelte`, `AppFailureDecision.svelte`, `LauncherChooserGuide.svelte`, `ProtectionSummary.svelte`, and `UsbDebuggingGuide.svelte`.
- Extend workspace state and add focused Svelte tests.

**Interfaces consumed:** Transaction progress and decision events from Tasks 12 and 15.

**Work details:**

- Present ordered operation progress using the approved layout and persistent status treatment, with current step, verified completed steps, and specific recovery action on failure.
- For ordinary app suspension failure, name each affected app and present only Continue with those apps available or Roll back before HOME can change.
- For store failure, show rollback progress and explain that setup cannot continue with incomplete store protection; do not offer an override.
- For shell HOME failure, guide the exact Android launcher chooser interaction, wait for the user to continue, and show success only after backend verification.
- Distinguish optional sideload gaps from successfully blocked paths and never call an unverified app or source blocked.
- On success, guide disabling USB debugging and optionally Developer Options. Use the universal banking, government, workplace, and sensitive-app warning without maintaining an app-name list.

**Verification:**

- UI tests cover success, slow progress, ordinary continue, ordinary rollback, store hard failure, optional warning, disconnect, resume, HOME chooser, chooser cancellation, rollback failure, and completion.
- Apply cannot be started twice, bypassed by navigation, or marked complete by frontend state alone.
- Focus moves to each decision heading, live announcements do not repeat the full journal, and controls remain keyboard operable.
- Completion appears only after the backend reports resolved HOME and verified policy state.

**Acceptance:** Users can follow every apply outcome safely, including the two approved manual decisions, without interpreting ADB output.

**Commit:** `feat: add guided policy application UI`

### Task 19: Build active-policy, maintenance, restore, and diagnostics UI

**Depends on:** Tasks 13-16 and 18.

**Files:**

- Create `desktop/src/lib/screens/ActivePolicyScreen.svelte`, `MaintenanceScreen.svelte`, `RestoreScreen.svelte`, and `DiagnosticsScreen.svelte`.
- Create `desktop/src/lib/components/TypedConfirmation.svelte`, `MaintenanceWarning.svelte`, `RestoreSummary.svelte`, `DiagnosticPreview.svelte`, and `RecoveryBlock.svelte`.
- Extend workspace state and add focused Svelte tests.

**Interfaces consumed:** Reconciled session, edit, maintenance, restore, cleanup retry, and diagnostic DTOs from Task 15.

**Work details:**

- Route a reconciled active policy to Edit allowed apps, Store maintenance, and Restore phone instead of creating a new baseline.
- Reuse the Task 17 chooser for edit while clearly showing only the planned delta and preserving the original baseline.
- Require the maintenance warning before opening, show the connected open state prominently, explain that new installs are possible, and intercept ordinary close until verified maintenance close completes.
- Require exact typed restore confirmation, list recorded changes, show inverse progress, support launcher chooser fallback, and retain retry actions when cleanup is incomplete.
- When histories are forked, corrupt, mismatched, missing, or unexplained, show only safe recovery guidance and diagnostic export; never surface a force button.
- Preview diagnostic contents and let the native save dialog select the destination before writing locally.

**Verification:**

- Tests cover each reconciled session class, edit delta, maintenance warning, exit interception, maintenance recovery, confirmation mismatch, restore progress, cleanup retry, blocked inconsistency, missing envelopes, and diagnostic preview.
- A window-close integration test proves maintenance close is requested and failure keeps the user informed rather than silently exiting.
- No screen offers policy editing or restore until reconciliation succeeds.
- The visual system remains the approved Guided Workspace rather than becoming a dashboard.

**Acceptance:** Every post-setup workflow is understandable, frictionful where intended, and constrained to actions the recovery engine has proven safe.

**Commit:** `feat: add policy management and restore UI`

### Task 20: Complete automated integration and compatibility coverage

**Depends on:** Tasks 4-19.

**Files:**

- Create end-to-end fake-backend scenarios under `desktop/src/tests/scenarios/`.
- Create Android emulator test configuration under `launcher/` and CI matrix entries in `.github/workflows/ci.yml` for API 24, 28, 29, 33, and 36.
- Create `docs/compatibility/device-matrix.md` and `docs/compatibility/manual-test-protocol.md`.

**Work details:**

- Exercise the whole desktop workflow against the Rust fake adapter rather than adding a second mock transaction implementation in TypeScript.
- Cover install and inspect, real-icon catalog, select and review, apply, interruption and resume, ordinary failure choice, store rollback, HOME chooser, edit, maintenance, launcher data clear, fresh-desktop recovery, USB-debugging disable and re-enable, restore, and cleanup retry.
- Run launcher filtering, bridge security, schema, and HOME behavior on each required emulator API.
- Execute the manual protocol on at least one Pixel-class, one Samsung, and one Xiaomi or Redmi device before using the broad V1 marketing claim.
- Record recents and gesture navigation with the baseline launcher unsuspended, notification suppression for a blocked app, MIUI guidance, networking-disabled operation, and out-of-scope profiles.
- Keep hardware results local and manually curated into the compatibility table; do not add telemetry collection.

**Verification:**

- All Rust, Svelte, Kotlin, provider instrumentation, cross-language fixture, fake end-to-end, and emulator-matrix tests pass from a clean checkout.
- The fresh-desktop recovery scenario uses only the phone's valid shared envelope, not host-local test state.
- Manual records include evidence for every clean-machine acceptance item and clearly label untested combinations.
- Failures in required store suspension, bridge security, recovery reconciliation, or HOME verification block release rather than being marked flaky.

**Acceptance:** Automated and recorded real-device evidence covers every V1 invariant and the exact API matrix promised by the specification.

**Commit:** `test: complete Unscroll V1 compatibility coverage`

### Task 21: Package, harden, sign, and accept the Windows release

**Depends on:** Tasks 2 and 20.

**Files:**

- Modify `desktop/src-tauri/tauri.conf.json` and Tauri capability files for the final NSIS bundle.
- Create release automation under `.github/workflows/release.yml` and resource staging under `scripts/`.
- Create `docs/release/windows.md`, `docs/release/signing.md`, `docs/release/acceptance-checklist.md`, and the final user-facing recovery guidance in `README.md`.
- Update `THIRD_PARTY_NOTICES.md` with the exact shipped artifacts.

**Work details:**

- Bundle the compatible launcher APK and verified ADB runtime inside one Windows 10/11 x64 NSIS installer with Start-menu and uninstall entries.
- Ensure installed users need no Node, Rust, Android SDK, browser, Docker, WSL, or manual PATH changes.
- Restrict Tauri capabilities to the reviewed command surface and package only expected binaries, licenses, icons, and frontend assets.
- Link to precise manufacturer driver guidance without silently installing unsigned or vendor-specific drivers.
- Integrate Authenticode signing through an approved FOSS path such as SignPath Foundation. Label unsigned development builds prominently and never publish them as a consumer release.
- Generate checksums and a software bill of materials for release artifacts and verify the stable launcher signing identity before upgrade installation.
- Run the full acceptance checklist on clean Windows 10 and Windows 11 x64 machines, including an offline repeat after installation.

**Verification:**

- Fresh virtual machines install, launch, operate, restore, and uninstall without development tools or network access after installation.
- Installer inspection finds only pinned, checksummed resources and complete third-party notices.
- Windows signature verification succeeds for public artifacts; an unsigned build is visibly marked and rejected by the public-release job.
- The acceptance checklist covers disconnected and unauthorized guidance, icons, apply, blocked notifications, baseline recents and gestures, interruption, edit, maintenance, fresh-desktop restore, Developer Options, MIUI guidance, keyboard, Narrator, and offline operation.

**Acceptance:** A signed, self-contained, honest Windows x64 release satisfies the complete V1 specification and publishes only capability-tested support claims.

**Commit:** `build: package Unscroll V1 for Windows`

## Final release gate

- Every task commit has passed specification review and quality review in order.
- All automated suites and the API 24, 28, 29, 33, and 36 emulator matrix pass from the release commit.
- Pixel-class, Samsung, and Xiaomi or Redmi manual evidence is recorded; missing evidence narrows the published compatibility claim.
- The baseline launcher remains installed, unsuspended, and hidden through apply, edit, maintenance, reconnect, and restore testing.
- Both recovery copies survive pending and applied interruptions, strict extensions repair safely, and every ambiguous history blocks mutation.
- Store failures roll back, ordinary app failures pause before HOME, and optional sideload gaps are described accurately.
- A fresh desktop installation restores after launcher data clearing using the shared envelope.
- The approved Guided Workspace HTML is reflected consistently across every desktop state, with real phone icons only in the chooser and a text-only phone launcher.
- Public artifacts are signed, checksummed, licensed, offline-capable, and verified on clean Windows 10 and 11 x64 systems.

Implementation stops at this gate. Linux, macOS, multiple launchers, stronger Device Owner provisioning, multi-profile support, and automatic-update separation require separately approved specifications and plans.
