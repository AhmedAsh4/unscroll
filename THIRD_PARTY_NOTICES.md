# Third-party notices

## Olauncher

`launcher/` is a source snapshot of [Olauncher](https://github.com/tanujnotes/Olauncher) at commit `952d9e942170a57583d0885a677e6f35c841b2ae`, licensed GPL-3.0-only. Its complete upstream license is retained at [launcher/LICENSE](launcher/LICENSE), and import provenance is recorded in [launcher/UPSTREAM.md](launcher/UPSTREAM.md).

## Desktop dependencies

The desktop scaffold uses Tauri (MIT or Apache-2.0), Svelte (MIT), Vite (MIT), and TypeScript (Apache-2.0). Exact resolved versions are recorded in the committed npm and Cargo lockfiles. Their license notices remain available in their published packages and source distributions.

## Android Debug Bridge

Release preparation obtains the unmodified `adb.exe`, `AdbWinApi.dll`, and
`AdbWinUsbApi.dll` subset from Google's official Android SDK Platform-Tools
37.0.1 Windows archive. These AOSP components and their bundled dependencies are
distributed under the open-source terms retained verbatim in
[`third_party/adb/LICENSE`](third_party/adb/LICENSE). Exact provenance, hashes,
and the redistribution decision are recorded in
[`docs/compatibility/adb-distribution-decision.md`](docs/compatibility/adb-distribution-decision.md).
