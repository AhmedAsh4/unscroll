# Unscroll V1 release signing

## Desktop (Authenticode via SignPath Foundation)

Unscroll uses the [SignPath Foundation](https://signpath.org/) free
code-signing path for open-source projects. What gets signed: the
application `.exe` and the NSIS installer produced by `npm run tauri build`.

### One-time project setup

1. Request code signing on the SignPath Foundation site and register the
   Unscroll project as a free open-source project.
2. Create a signing policy for Windows release builds (Authenticode).
3. Add these repository secrets (Settings, Secrets and variables, Actions):
   - `SIGNPATH_API_TOKEN`: CI API token for submitting signing requests.
   - `SIGNPATH_ORGANIZATION_ID`: your SignPath organization id.
   - `SIGNPATH_PROJECT_SLUG`: the registered project slug.
   - `SIGNPATH_SIGNING_POLICY_SLUG`: the release signing policy slug.

### CI behavior (`.github/workflows/release.yml`)

- When the `SIGNPATH_*` secrets are present, the `sign` job submits the
  NSIS output through `SignPath/github-action-submit-signing-request` and
  publishes only the signed result.
- When they are absent, the build is renamed to `*-unsigned-dev*`,
  labeled `UNSIGNED_BUILD=1`, and the `verify` job rejects it on any
  version tag (`v*`). The `publish` job FAILS while unsigned, so an
  unsigned build can never become a consumer release.

### How to verify a download

- PowerShell: `Get-AuthenticodeSignature .\Unscroll_<version>_x64-setup.exe`
  must report `Status: Valid` with the Unscroll publisher certificate.
- Or: right-click the file, Properties, Digital Signatures tab.
- Cross-check the file hash against the release `SHA256SUMS`.

## Launcher APK (stable Android signing identity)

The launcher APK is built from source at release time
(`gradlew assembleRelease`) and staged with its expected signing identity
in `desktop/src-tauri/resources/launcher/unscroll-launcher.apk.sha256`
(see `scripts/stage-release.ps1`). At inspect time the desktop compares that
sidecar against the bridge-reported `launcher_signing_sha256`
(`src/device/inspect.rs`); a mismatch fails closed before any device write.

Consequence: the release signing identity is stable. Rotating the APK
signing key requires a coordinated desktop + launcher release (new APK,
new sidecar, new bridge build), never a silent swap.
