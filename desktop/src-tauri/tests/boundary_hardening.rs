//! RED (TDD) for the pre-Task-18 boundary-hardening pass.
//!
//! Covers the three fixes; every test must FAIL before the fix lands:
//! - FIX 1: structured progress payloads (`progress_payload` builder).
//! - FIX 2: bounded `load_app_icon` (binding/package/bounds/miss + peek).
//! - FIX 3: store flags in the catalog DTO.

use unscroll_desktop_lib::{
    app_state::UnscrollState,
    commands::{
        device as device_edge,
        handlers::{self, APP_ENTRY_DTO_FIELDS, COMMAND_NAMES},
    },
    device::{DeviceSnapshot, ProfileFact},
};

fn minimal_snapshot() -> DeviceSnapshot {
    use unscroll_desktop_lib::adb::{Component, PackageId};
    use unscroll_desktop_lib::device::{
        AppCatalogEntry, CapabilityReport, InstallSourceFact, RecoveryObservation, StoreFact,
    };
    let entry = |package: &str, label: &str| AppCatalogEntry {
        package: PackageId::parse(package).unwrap(),
        component: Component::parse(&format!("{package}/.Main")).unwrap(),
        label: label.to_owned(),
        icon: Vec::new(),
        suspended: false,
        enabled: true,
    };
    DeviceSnapshot {
        serial: "serial-a".into(),
        api: 34,
        model: "Pixel".into(),
        manufacturer: "Google".into(),
        fingerprint: "fp/fp-a".into(),
        current_user: 0,
        home: Component::parse("com.base/.Home").unwrap(),
        catalog: vec![entry("com.example.maps", "Maps"), entry("com.android.vending", "Play Store")],
        protected: vec![],
        stores: vec![StoreFact {
            package: PackageId::parse("com.android.vending").unwrap(),
        }],
        install_sources: vec![InstallSourceFact {
            package: PackageId::parse("com.android.vending").unwrap(),
            allowed: true,
        }],
        profiles: vec![ProfileFact { user_id: 0, name: String::new() }],
        private_recovery: RecoveryObservation::Missing,
        shared_recovery: RecoveryObservation::Missing,
        xiaomi_guidance: vec![],
        capabilities: CapabilityReport {
            package_suspension: true,
            bridge: true,
            recovery_storage: true,
            app_op_inspection: true,
            home_path: true,
        },
    }
}

// --- FIX 1: structured progress payloads ------------------------------------

#[test]
fn progress_payload_carries_event_serial_and_detail() {
    let text = handlers::progress_payload("decision-required", Some("serial-a"), Some("com.example.maps"));
    assert!(text.contains("\"event\":\"decision-required\""), "event missing: {text}");
    assert!(text.contains("\"serial\":\"serial-a\""), "serial missing: {text}");
    assert!(text.contains("\"detail\":\"com.example.maps\""), "detail missing: {text}");
    assert!(text.starts_with('{') && text.ends_with('}'), "must be a JSON object: {text}");
}

#[test]
fn progress_payload_uses_null_when_serial_or_detail_absent() {
    let text = handlers::progress_payload("disconnected", Some("serial-a"), None);
    assert!(text.contains("\"event\":\"disconnected\""), "event missing: {text}");
    assert!(text.contains("\"serial\":\"serial-a\""), "serial missing: {text}");
    assert!(text.contains("\"detail\":null"), "detail must be null: {text}");
    let bare = handlers::progress_payload("started", None, None);
    assert!(bare.contains("\"serial\":null"), "serial must be null: {bare}");
    assert!(bare.contains("\"detail\":null"), "detail must be null: {bare}");
}

#[test]
fn progress_payload_escapes_embedded_quotes() {
    let text = handlers::progress_payload("started", Some("a\"b"), Some("c\\d"));
    assert!(text.contains("\\\""), "quotes must be escaped: {text}");
    assert!(text.contains("\\\\"), "backslashes must be escaped: {text}");
}

// --- FIX 2: bounded load_app_icon --------------------------------------------

#[test]
fn peek_icon_does_not_remove_the_cached_entry() {
    let state = UnscrollState::new();
    state.store_icon("com.example.maps", vec![1, 2, 3]).unwrap();
    assert_eq!(state.peek_icon("com.example.maps"), Some(vec![1, 2, 3]));
    // A row re-render must not lose the icon: the entry is still cached.
    assert_eq!(state.peek_icon("com.example.maps"), Some(vec![1, 2, 3]));
    assert_eq!(state.icon_count(), 1);
}

#[test]
fn store_icon_rejects_empty_payloads() {
    // Empty bytes are not a renderable icon: storing them must fail closed
    // with the same bound code the load path reports, so an empty store
    // can never become a cached entry that later fails at load time.
    let state = UnscrollState::new();
    assert_eq!(state.store_icon("com.app", Vec::new()).unwrap_err().code, "icon-too-large");
    assert_eq!(state.icon_count(), 0);
    assert_eq!(handlers::icon_data_url(&[]).unwrap_err().code, "icon-too-large");
}

#[test]
fn load_icon_rejects_stale_binding_and_bad_packages() {
    let state = UnscrollState::new();
    device_edge::bind_session(&state, "serial-a", "fp/fp-a").unwrap();
    state.store_icon("com.example.maps", vec![137, 80, 78, 71]).unwrap();
    let stale = handlers::load_icon_data_url(&state, "serial-b", "fp/fp-a", "com.example.maps");
    let stale_err = stale.unwrap_err();
    assert!(stale_err.contains("stale-device"), "stale binding must fail typed: {stale_err}");
    let bad = handlers::load_icon_data_url(&state, "serial-a", "fp/fp-a", "not a package!!");
    assert!(bad.unwrap_err().contains("invalid-package-id"), "bad package must fail typed");
}

#[test]
fn load_icon_returns_data_url_and_typed_miss() {
    let state = UnscrollState::new();
    device_edge::bind_session(&state, "serial-a", "fp/fp-a").unwrap();
    // Minimal PNG header bytes; the data URL must be image/png base64.
    state.store_icon("com.example.maps", vec![137, 80, 78, 71, 13, 10, 26, 10]).unwrap();
    let hit = handlers::load_icon_data_url(&state, "serial-a", "fp/fp-a", "com.example.maps").unwrap();
    assert!(hit.starts_with("data:image/png;base64,"), "must be a PNG data URL: {hit}");
    // A missing cache entry is a typed miss (Ok), never a loud error.
    let miss = handlers::load_icon_data_url(&state, "serial-a", "fp/fp-a", "com.example.gone").unwrap();
    assert!(miss.contains("missing"), "miss must be explicit: {miss}");
    assert!(!miss.starts_with("data:"), "miss must not look like an icon: {miss}");
}

#[test]
fn icon_data_url_rejects_oversize_bytes() {
    let big = vec![0u8; unscroll_desktop_lib::app_state::MAX_ICON_BYTES + 1];
    let error = handlers::icon_data_url(&big).unwrap_err();
    assert_eq!(error.code, "icon-too-large");
}

#[test]
fn load_app_icon_is_registered_with_command_parity() {
    assert!(COMMAND_NAMES.contains(&"load_app_icon"), "COMMAND_NAMES must list load_app_icon");
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let invoke = std::fs::read_to_string(manifest.join("../src/lib/api/invoke.ts"))
        .expect("frontend transport must exist");
    assert!(invoke.contains("\"load_app_icon\""), "invoke.ts must call load_app_icon");
    let lib = std::fs::read_to_string(manifest.join("src/lib.rs")).expect("lib.rs must exist");
    assert!(lib.contains("load_app_icon"), "lib.rs must register load_app_icon");
}

// --- FIX 3: store flags in the catalog DTO -----------------------------------

#[test]
fn catalog_dto_fields_include_store_flags() {
    assert!(APP_ENTRY_DTO_FIELDS.contains(&"isStore"), "missing isStore field");
    assert!(APP_ENTRY_DTO_FIELDS.contains(&"isInstallSource"), "missing isInstallSource field");
}

#[test]
fn catalog_json_marks_store_and_install_source_entries() {
    let snapshot = minimal_snapshot();
    let json = handlers::catalog_json(&snapshot, &[false, false]);
    // Play Store entry carries both flags; Maps carries neither.
    let vending = json.find("com.android.vending").expect("vending entry must exist");
    let window = &json[vending..(vending + 400).min(json.len())];
    assert!(window.contains("\"isStore\":true"), "vending must be flagged as store: {window}");
    assert!(window.contains("\"isInstallSource\":true"), "vending must be flagged as source: {window}");
    let maps = json.find("com.example.maps").expect("maps entry must exist");
    let window = &json[maps..(maps + 400).min(json.len())];
    assert!(window.contains("\"isStore\":false"), "maps must not be flagged: {window}");
    assert!(window.contains("\"isInstallSource\":false"), "maps must not be flagged: {window}");
}

#[test]
fn typescript_app_entry_dto_requires_store_flags() {
    let types = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/lib/api/types.ts"),
    )
    .expect("frontend types must exist");
    assert!(types.contains("isStore"), "types.ts must declare isStore");
    assert!(types.contains("isInstallSource"), "types.ts must declare isInstallSource");
}
