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
(`gradlew assembleRelease`), signed with the stable release key, and staged
with its expected signing identity in
`desktop/src-tauri/resources/launcher/unscroll-launcher.apk.sha256`
(see `scripts/stage-release.ps1`). At inspect time the desktop compares that
sidecar against the bridge-reported `launcher_signing_sha256`
(`src/device/inspect.rs`); a mismatch fails closed before any device write.

### Release key custody (read this before v0.1.0)

- Keystore: `launcher/unscroll-release.jks` (PKCS12, RSA 4096, alias
  `unscroll`, self-signed, 30-year validity). It is git-ignored and exists
  in exactly two places: the maintainer's offline backup and the
  `LAUNCHER_KEYSTORE_BASE64` repository secret.
- Back up the `.jks` file AND the store password offline, somewhere that
  survives losing this machine. Losing either one permanently ends this
  signing identity: existing installs would refuse updates signed by a new
  key, and every desktop release would fail its bridge signature check.
- Nobody else needs the key. Contributors build unsigned `assembleRelease`
  output by default (the signing config applies only when the keystore is
  present); CI signs only from the secret.

### CI secrets for APK signing

| Secret | Content |
| ------ | ------- |
| `LAUNCHER_KEYSTORE_BASE64` | Base64 of `launcher/unscroll-release.jks` |
| `LAUNCHER_STORE_PASSWORD` | Keystore (store) password |
| `LAUNCHER_KEY_ALIAS` | Key alias (`unscroll`) |
| `LAUNCHER_KEY_PASSWORD` | Key password (same as store password: PKCS12 ignores a distinct value) |

`release.yml` restores the keystore from the secret, passes the passwords to
Gradle through `UNSCROLL_*` environment variables (never committed, never
logged — GitHub masks secret values), and then fails closed at
`apksigner verify` if the APK is unsigned or the identity is unreadable.
Local builds use the same `UNSCROLL_*` variables or a git-ignored
`launcher/keystore.properties` with `storeFile`, `storePassword`,
`keyAlias`, and `keyPassword` entries.

Consequence: the release signing identity is stable. Rotating the APK
signing key requires a coordinated desktop + launcher release (new APK,
new sidecar, new bridge build), never a silent swap.
