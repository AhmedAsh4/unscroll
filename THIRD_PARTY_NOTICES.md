# Third-party notices

## Olauncher

`launcher/` is a source snapshot of [Olauncher](https://github.com/tanujnotes/Olauncher) at commit `952d9e942170a57583d0885a677e6f35c841b2ae`, licensed GPL-3.0-only. Its complete upstream license is retained at [launcher/LICENSE](launcher/LICENSE), and import provenance is recorded in [launcher/UPSTREAM.md](launcher/UPSTREAM.md).

## Desktop dependencies

The desktop scaffold uses Tauri (MIT or Apache-2.0), Svelte (MIT), Vite (MIT), and TypeScript (Apache-2.0). Exact resolved versions are recorded in the committed npm and Cargo lockfiles. Their license notices remain available in their published packages and source distributions.

Shipped V1 set (pinned, from `desktop/package-lock.json` and `desktop/src-tauri/Cargo.lock`):

- Tauri 2.11.5 (Rust crates) / `@tauri-apps/api` 2.11.1 / `@tauri-apps/cli` 2.11.4
- Svelte 5.57.0, Vite 7.3.6, TypeScript 5.9.3

No new runtime dependencies were added for the release: the installed app
needs only the OS WebView2 component inbox on Windows 10/11, plus the
pinned files below. No Node, Rust, SDK, browser, Docker, WSL, or PATH
requirement ships with the installer.

## Android Debug Bridge

Release preparation obtains the unmodified `adb.exe`, `AdbWinApi.dll`, and
`AdbWinUsbApi.dll` subset from Google's official Android SDK Platform-Tools
37.0.1 Windows archive. These AOSP components and their bundled dependencies are
distributed under the open-source terms retained verbatim in
[`third_party/adb/LICENSE`](third_party/adb/LICENSE). Exact provenance, hashes,
and the redistribution decision are recorded in
[`docs/compatibility/adb-distribution-decision.md`](docs/compatibility/adb-distribution-decision.md).

Shipped ADB set (exact, verified by `scripts/prepare-adb.ps1` and
`scripts/stage-release.ps1`; hashes from `third_party/adb/checksums.txt`):

- Archive `platform-tools_r37.0.1-win.zip` SHA-256
  `45F4D63113E895EBDE0C90F194099A4676B6AC653BD28D54314A9E022BBC1A99`
- `adb.exe` SHA-256 `B4A6B455702684652CCCF7B46258B29E653538904359A58FD4931CF3EF286B3F`
- `AdbWinApi.dll` SHA-256 `C1D653030B4BDE65D3E07E4D0B0979E17BE56DF1436CDD15528630F27808050D`
- `AdbWinUsbApi.dll` SHA-256 `0710E894D9B40F71A670C13C694079D564C92C1279DA382CFE4850983AAEBE1B`
- Upstream notice retained at `third_party/adb/LICENSE`.
- WOW64 note: each file is PE machine I386 (`0x014C`), run through WOW64 on
  64-bit x64 Windows only; ARM64 hosts fail closed (see the decision document).

## Launcher APK (built at release, never committed)

The installer bundles one release APK built from this repository's
`launcher/` source (`org.unscroll.launcher`, `Unscroll Launcher 1.0`,
versionCode 111) plus its detached signing-identity sidecar
(`unscroll-launcher.apk.sha256`). Olauncher remains GPL-3.0-only at commit
`952d9e942170a57583d0885a677e6f35c841b2ae` with its license at
[launcher/LICENSE](launcher/LICENSE).
