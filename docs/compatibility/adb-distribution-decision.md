# Windows ADB distribution decision

## Decision

Unscroll pins the unmodified ADB subset from Google's official Android SDK
Platform-Tools 37.0.1 Windows archive:

- immutable URL: <https://dl.google.com/android/repository/platform-tools_r37.0.1-win.zip>
- Google repository SHA-1: `e03e78b1d80b396f1c3358e31251cb31740e1110`
- independently pinned SHA-256: `45F4D63113E895EBDE0C90F194099A4676B6AC653BD28D54314A9E022BBC1A99`
- reported ADB version: `Android Debug Bridge version 1.0.41`, `Version 37.0.1-15733141`
- packaged file set: `adb.exe`, `AdbWinApi.dll`, `AdbWinUsbApi.dll`

The archive remains downloaded build input. It and the extracted runtime files
are ignored by Git; release preparation obtains and verifies them with
`scripts/prepare-adb.ps1`.

## Redistribution review

Primary evidence reviewed on 2026-08-30:

1. The [Android SDK License Agreement](https://developer.android.com/studio/terms)
   section 3.4 generally restricts SDK redistribution, while section 3.5 says
   components licensed under open-source licenses are governed solely by those
   licenses.
2. Google's [Platform-Tools release notes](https://developer.android.com/tools/releases/platform-tools)
   identify 37.0.1 as the current official release.
3. Google's [SDK repository metadata](https://dl.google.com/android/repository/repository2-3.xml)
   identifies the immutable Windows archive, size `8044989`, and published SHA-1.
4. The archive's `NOTICE.txt` begins with Apache License 2.0, includes its
   object-form redistribution grant and conditions, and retains the bundled
   attributions/license material. Its exact contents are committed at
   `third_party/adb/LICENSE`.
5. AOSP adb's [`Android.bp`](https://android.googlesource.com/platform/packages/modules/adb/+/refs/tags/android-17.0.0_r1/Android.bp)
   declares `SPDX-license-identifier-Apache-2.0` and `NOTICE` as its license text.

On that primary evidence, the unmodified ADB object files are redistributed
under their included open-source terms, with the complete upstream notice
retained. This decision does not claim that the complete Android SDK is
redistributable.

## Windows x64 host compatibility

Google publishes one official Windows Platform-Tools archive. Each selected
37.0.1 file has PE machine I386 (`0x014C`); the files are 32-bit and are not
native AMD64. Unscroll supports them only on a 64-bit x64 Windows host through
WOW64. Microsoft documents WOW64 as the compatibility environment that runs
32-bit applications on 64-bit Windows in
[_Running 32-bit Applications_](https://learn.microsoft.com/en-us/windows/win32/winprog64/running-32-bit-applications).

Preparation fails unless the OS is 64-bit x64 Windows and every file's PE
machine field equals its pinned `0x014C` value. An ARM64 Windows host or a future
file architecture requires a new compatibility review and normal pin upgrade.

## Reproducibility and fail-closed boundary

The preparer uses only PowerShell and .NET. Before producing packaging input it
checks:

- the committed license/notice SHA-256;
- the immutable archive SHA-256;
- exact, case-sensitive archive entry names and absence of duplicate required entries;
- the archive notice and `source.properties` SHA-256;
- the exact three-file staged runtime set and filenames;
- x64 Windows host and each pinned I386 PE machine field;
- each runtime file SHA-256;
- the exact reported ADB version with Android SDK environment variables removed,
  only after the runtime checksums pass.

Verification repeats the runtime boundary against already staged files.
Tampering, missing/renamed/extra files, architecture changes, version changes,
license changes, and checksum changes stop before packaging succeeds.

## Upgrade ownership and required evidence

The desktop/release owner owns this pin. A version change requires one reviewed
change containing all of the following release evidence:

1. official Google release notes and immutable repository metadata;
2. fresh review of the then-current SDK terms and complete archive notice;
3. immutable archive SHA-256 and SHA-256 for every shipped EXE/DLL;
4. exact staged filenames, PE machine values, host/WOW64 compatibility decision,
   and reported version;
5. two clean Windows preparations with byte-identical runtime checksums;
6. negative results for archive, filename, complete file set, PE machine,
   version, runtime checksum, and license tampering;
7. proof that staged `adb version` starts without Android Studio or a separately
   installed Android SDK;
8. synchronized `LICENSE`, checksums, third-party notice, decision document,
   preparer pins, and installer resource configuration.

Unresolved licensing, architecture, provenance, or reproducibility evidence
blocks packaging.
