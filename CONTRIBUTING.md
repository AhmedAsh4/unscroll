# Contributing to Unscroll

All Unscroll-authored contributions must be GPL-3.0-only and keep required third-party notices intact.

Use the committed lockfiles for reproducible builds:

Prerequisites: Node.js 24, Rust stable, a JDK 17+ with `JAVA_HOME` set to its root, and an Android SDK with API 36 plus Build Tools 36 installed with `ANDROID_HOME` set to its root.

```powershell
npm ci --prefix desktop
npm run check --prefix desktop
npm run build --prefix desktop
cargo build --manifest-path desktop/src-tauri/Cargo.toml --locked
Push-Location launcher
try {
  .\gradlew.bat --no-daemon :app:testDebugUnitTest :app:assembleDebug
} finally {
  Pop-Location
}
```

Do not rebrand, reduce, or otherwise change the imported Olauncher behavior before its dedicated task.
