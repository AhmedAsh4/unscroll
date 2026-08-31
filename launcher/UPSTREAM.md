# Olauncher upstream provenance

- Repository: https://github.com/tanujnotes/Olauncher
- Pinned commit: `952d9e942170a57583d0885a677e6f35c841b2ae`
- Imported date: 2026-08-30
- Retained license: GPL-3.0-only; the upstream `LICENSE` and all copyright and license notices remain in `launcher/`.

## Local Task 1 changes

- Changed only the Android `applicationId` to the stable `org.unscroll.launcher`; the imported `app.olauncher` source namespace, visible brand, and behavior remain unchanged.
- No Android SDK values changed: the pinned upstream snapshot already uses minimum API 24 and compile/target API 36.
- Enabled Gradle dependency locking and added `settings-gradle.lockfile` plus `app/gradle.lockfile`.

## Future updates

1. Fetch the upstream repository and choose a reviewed immutable commit.
2. Verify the checked-out commit hash before copying files.
3. Replace this snapshot without a nested `.git` directory while retaining upstream licenses, notices, and this provenance record.
4. Update this file with the repository URL, commit, date, and any local changes, then regenerate and review the Gradle lockfiles.

## Local Task 4 changes

- Rebranded the visible application as Unscroll Launcher and replaced the launcher icon.
- Moved maintained launcher code to `org.unscroll.launcher` and reduced it to text home/app lists, search, app launching, shortcut pin acknowledgement, and a local text-size preference.
- Removed the imported hidden-app editor, policy controls, usage statistics, online wallpaper and promotional features, Accessibility Service, Device Admin, and their permissions/components. No Unscroll policy store or bridge is present at this stage.
