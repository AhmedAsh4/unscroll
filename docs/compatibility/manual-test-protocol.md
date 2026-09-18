# Unscroll V1 manual test protocol

Run on a clean Windows 10/11 x64 machine without development tools, one
supported phone at a time, over USB (wireless ADB is out of scope). Record
every step locally; nothing is uploaded (no telemetry). Each step lists
setup, action, and expected evidence. Evidence fields for every step: phone
model, build fingerprint, API level, observed result, limitation.

Out-of-scope profiles (record, do not test through): work profiles, multiple
users beyond the primary user, Device Owner/kiosk setups, and
accessibility-service enforcement. If the phone carries any of these, note it
under limitations and use a primary-user-only device instead.

MIUI/HyperOS prerequisites (Xiaomi/Redmi): a signed-in Mi account, an
inserted SIM card, or a network connection may be required by the
manufacturer before `Install via USB` can turn on. Turn on `Install via USB`
in Developer options and accept the prompt on the phone. These are
manufacturer requirements and cannot be skipped from the desktop.

## Steps (the 14 clean-machine acceptance items)

### 1. Install and launch from the normal installer

- Setup: clean Windows 10/11 x64, signed installer, one supported phone unplugged.
- Action: install, launch from the Start-menu entry, confirm the unsigned-build label is absent.
- Expected evidence: model, build fingerprint, API, installer checksum match, app launches offline; limitation or none.

### 2. Disconnected and unauthorized guidance

- Setup: app open, phone unplugged; then plugged without authorizing.
- Action: check for the phone in each state.
- Expected evidence: `No phone found` then `Phone is waiting for authorization` guidance with the adjacent recovery action; limitation or none.

### 3. Real apps and icons

- Setup: one authorized phone connected over USB.
- Action: inspect and open the app chooser.
- Expected evidence: real installed apps with real phone icons; missing icons show the neutral initial-letter fallback (never guessed brand art); launcher stays text-only; limitation or none.

### 4. Allowlist apply without data loss

- Setup: inspected phone, allowlist chosen (protected entries locked).
- Action: review, confirm, apply; open a kept app and check its data.
- Expected evidence: kept apps launch with data intact; nothing uninstalled or cleared; limitation or none.

### 5. Blocked app absent, unlaunchable, suspended, notifications suppressed

- Setup: policy applied with at least one ordinary app blocked.
- Action: look for the blocked app in Unscroll Launcher, try to launch it normally, send it a test notification.
- Expected evidence: absent from Unscroll Launcher, cannot launch normally, suspended, no notification surfaces; limitation or none.

### 6. Baseline launcher unsuspended and hidden; recents and gestures work

- Setup: policy applied.
- Action: confirm the baseline launcher is unsuspended and hidden from Unscroll Launcher; use recents and gesture navigation.
- Expected evidence: baseline unsuspended, hidden, recents and gestures behave as on the unmodified phone; limitation or none.

### 7. Both HOME paths (shell and guided chooser fallback)

- Setup: policy applied via the HOME shell path on one run; forced chooser fallback on another (or a second device).
- Action: complete setup through each path.
- Expected evidence: verified default launcher on both paths; chooser steps completable by hand; limitation or none.

### 8. Interrupt and recover

- Setup: apply running.
- Action: unplug mid-apply, reconnect, reconcile (resume or rollback).
- Expected evidence: pending journal mirrored on both copies, resume completes or rollback verifies unchanged; limitation or none.

### 9. Apply, edit, clear data, fresh-desktop restore via the shared journal

- Setup: applied policy, then an edit; then clear Unscroll Launcher data on the phone.
- Action: from a fresh desktop installation (no host state), reconcile from the shared envelope only and restore.
- Expected evidence: recovery uses `/sdcard/Documents/Unscroll/recovery-v1.json` only; restore completes and verifies; limitation or none.

### 10. Store maintenance and re-block

- Setup: active policy.
- Action: open store maintenance with the exact phrase, install an unapproved app, close maintenance.
- Expected evidence: warning shown, exit intercepted while open, close rescans and re-blocks the new app, stores restricted again; limitation or none.

### 11. Disable Developer Options, verify persistence, re-enable and restore

- Setup: active policy.
- Action: disable Developer Options/USB debugging, verify the policy persists; re-enable debugging and restore the original state.
- Expected evidence: suspensions, HOME, and envelopes unchanged while disabled; restore gated until re-enabled, then completes; limitation or none.

### 12. MIUI/HyperOS bootstrap guidance

- Setup: Xiaomi/Redmi-class phone, prerequisites above met.
- Action: trigger a bootstrap restriction (`INSTALL_FAILED_USER_RESTRICTED` or equivalent).
- Expected evidence: documented actionable guidance naming `Install via USB` plus the Mi account/SIM/network requirement; limitation or none.

### 13. Keyboard and screen-reader flow

- Setup: Windows keyboard only, then Narrator/screen reader on.
- Action: complete connect, choose, review, and apply by keyboard; confirm announcements.
- Expected evidence: logical focus order, visible labels, single polite-region announcements, no color-only status, reduced-motion respected; limitation or none.

### 14. Offline repeat

- Setup: installer already installed; Windows networking disabled.
- Action: repeat install-launch-inspect-apply-restore with no network.
- Expected evidence: full flow works offline (no network fonts or fetches); limitation or none.

## Release-blocking rule

Store, bridge, reconciliation, or HOME failures block release and are never
marked flaky. Hardware results stay local and are curated by hand into
`device-matrix.md`; no upload, no telemetry.
