//! Task 21 release-packaging gate (focused, fast, no network, no device).
//!
//! Asserts the Windows release bundle surface without new dependencies:
//! - `tauri.conf.json`: active NSIS x64 bundle, expected identity, window
//!   geometry (1050 wide, usable at 800 without h-scroll), exactly the two
//!   resource mappings (`resources/adb/*` -> `adb/`,
//!   `resources/launcher/*` -> `launcher/`), empty `externalBin` (the dev
//!   probe binary `src/bin/unscroll_probe.rs` is never bundled), and no
//!   widened bundle targets or plugin permissions.
//! - `capabilities/default.json`: stays `core:default` only.
//!
//! The Rust loader paths are pinned elsewhere and must not move:
//! `BundledAdb::from_resource_dir(resource_dir/adb/adb.exe)`
//! (`src/adb/process.rs`) and the launcher pair
//! `resource_dir/launcher/unscroll-launcher.apk` + `.apk.sha256`
//! (`src/commands/handlers.rs` via `LauncherArtifact` in
//! `src/device/bootstrap.rs`).

use std::path::PathBuf;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_config(name: &str) -> String {
    let path = manifest_dir().join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing {name}"))
}

/// Strip all ASCII whitespace so assertions match exact compact JSON shapes.
fn flat(json: &str) -> String {
    json.chars().filter(|c| !c.is_whitespace()).collect()
}

fn tauri_conf() -> String {
    flat(&read_config("tauri.conf.json"))
}

fn capabilities() -> String {
    flat(&read_config("capabilities/default.json"))
}

/// Extract the flat text of the `"resources":{...}` object (brace-matched).
fn resources_block(conf: &str) -> String {
    let key = "\"resources\":{";
    let start = conf.find(key).expect("bundle.resources mapping is missing");
    let mut depth = 0usize;
    for (index, char) in conf[start + key.len() - 1..].char_indices() {
        match char {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return conf[start..start + key.len() - 1 + index + 1].to_string();
                }
            }
            _ => {}
        }
    }
    panic!("bundle.resources object is not closed");
}

#[test]
fn release_identity_is_pinned() {
    let conf = tauri_conf();
    assert!(
        conf.contains("\"productName\":\"Unscroll\""),
        "productName must be Unscroll"
    );
    assert!(
        conf.contains("\"version\":\"0.1.0\""),
        "release version must be 0.1.0"
    );
    assert!(
        conf.contains("\"identifier\":\"org.unscroll.desktop\""),
        "bundle identifier must be org.unscroll.desktop"
    );
}

#[test]
fn bundle_is_active_nsis_only() {
    let conf = tauri_conf();
    assert!(
        conf.contains("\"active\":true"),
        "bundle.active must be true for the Windows release"
    );
    assert!(
        conf.contains("\"targets\":[\"nsis\"]"),
        "bundle.targets must be exactly NSIS (Windows x64 only)"
    );
    assert!(
        conf.contains("\"nsis\":{"),
        "bundle.windows.nsis settings are required"
    );
    assert!(
        conf.contains("\"startMenuFolder\":\"Unscroll\""),
        "NSIS bundle must create a Start-menu entry"
    );
    for forbidden in ["\"wix\"", "\"msi\"", "\"appimage\"", "\"dmg\""] {
        assert!(
            !conf.contains(forbidden),
            "non-NSIS bundle target {forbidden} must not be configured"
        );
    }
}

#[test]
fn window_geometry_fits_narrow_screens() {
    let conf = tauri_conf();
    assert!(
        conf.contains("\"width\":1050"),
        "main window width must be 1050"
    );
    assert!(
        conf.contains("\"minWidth\":800"),
        "main window must stay usable at 800px without h-scroll"
    );
}

#[test]
fn resources_bundle_exactly_adb_and_launcher() {
    let conf = tauri_conf();
    let block = resources_block(&conf);
    assert!(
        block.contains("\"resources/adb/*\":\"adb/\""),
        "ADB runtime must bundle as resources/adb/* -> adb/"
    );
    assert!(
        block.contains("\"resources/launcher/*\":\"launcher/\""),
        "launcher APK must bundle as resources/launcher/* -> launcher/"
    );
    assert_eq!(
        block.matches("\":\"").count(),
        2,
        "bundle.resources must carry exactly the adb + launcher mappings, got: {block}"
    );
    assert!(
        !block.contains("target"),
        "bundle.resources must never reach into target/"
    );
}

#[test]
fn dev_probe_binary_is_excluded() {
    let conf = tauri_conf();
    assert!(
        conf.contains("\"externalBin\":[]"),
        "externalBin must stay empty so no dev binary ships"
    );
    assert!(
        !conf.contains("unscroll_probe"),
        "dev probe binary must not be referenced by the bundle"
    );
    assert!(
        !conf.contains("target/"),
        "build output directories must not be bundled"
    );
}

#[test]
fn no_extra_plugins_or_runtimes_implied() {
    let conf = tauri_conf();
    for forbidden in ["\"shell\"", "\"fs\"", "\"dialog\"", "\"updater\"", "\"http\""] {
        assert!(
            !conf.contains(forbidden),
            "bundle must not imply plugin {forbidden}"
        );
    }
}

#[test]
fn capabilities_stay_core_default_only() {
    let caps = capabilities();
    let key = "\"permissions\":[";
    let start = caps.find(key).expect("capabilities.permissions is missing");
    let rest = &caps[start + key.len()..];
    let end = rest.find(']').expect("capabilities.permissions is not closed");
    assert_eq!(
        &rest[..end],
        "\"core:default\"",
        "capabilities must stay core:default only"
    );
}
