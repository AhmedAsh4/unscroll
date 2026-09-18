# Unscroll V1 release acceptance checklist

Copy this list per release candidate. Every item needs observed evidence on
the release installer; unchecked assumptions never count. Fields per item:
result (pass/fail), phone model, build fingerprint, API level, limitation.

Capability claim rule: the broad "capability-tested Android 7-16 phones
(API 24-36)" claim may only ship when `docs/compatibility/device-matrix.md`
shows covering evidence. While any cell reads `Not-run`/`Not observed`,
narrow the published claim to what was actually tested.

## Installer and signatures

- [ ] Installer ships only pinned, checksummed resources (3 ADB files +
      launcher APK + signing sidecar match `SHA256SUMS`; `sbom.json` present).
- [ ] Authenticode signature verifies (`Get-AuthenticodeSignature` reports
      `Valid`; file-properties signer matches the release publisher).
- [ ] No `*-unsigned-dev*` build is published as a consumer release.

## Manual test protocol (14 items)

- [ ] 1. Install and launch from the normal installer (Start-menu entry,
      installer filename has no `*-unsigned-dev*` suffix — that suffix,
      created by release.yml, is the whole unsigned-build label; there is
      no in-app UI string — launches offline).
- [ ] 2. Disconnected and unauthorized guidance (`No phone found`, then
      `Phone is waiting for authorization` with the adjacent recovery action).
- [ ] 3. Real apps and icons (real installed apps, real phone icons, neutral
      initial-letter fallback, text-only launcher).
- [ ] 4. Allowlist apply without data loss (kept apps launch with data
      intact; nothing uninstalled or cleared).
- [ ] 5. Blocked app absent, unlaunchable, suspended, notifications suppressed.
- [ ] 6. Baseline launcher unsuspended and hidden; recents and gestures work.
- [ ] 7. Both HOME paths (shell path and guided chooser fallback both verify).
- [ ] 8. Interrupt and recover (unplug mid-apply; resume completes or rollback
      verifies unchanged; journal mirrored on both copies).
- [ ] 9. Apply, edit, clear data, fresh-desktop restore via the shared journal
      (`/sdcard/Documents/Unscroll/recovery-v1.json` only, from a fresh
      desktop install with no host state).
- [ ] 10. Store maintenance and re-block (warning shown, exit intercepted,
      close rescans and re-blocks, stores restricted again).
- [ ] 11. Disable Developer Options, verify persistence, re-enable and restore.
- [ ] 12. MIUI/HyperOS bootstrap guidance (`Install via USB` plus the
      Mi account/SIM/network requirement).
- [ ] 13. Keyboard and screen-reader flow (keyboard-only completion, Narrator
      announcements, no color-only status, reduced-motion respected).
- [ ] 14. Offline repeat (install, launch, inspect, apply, restore with no
      network).

## Sign-off

- Release tag:
- Installer checksum match (yes/no):
- Signature status:
- Tested API levels / device classes:
- Narrowed capability claim (if any):
- Signer:
