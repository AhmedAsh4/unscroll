//! Task 15 boundary tests for the narrow Tauri command layer.
//!
//! RED (TDD): these tests reference `app_state` and `commands`, which do not
//! exist yet. They must fail to compile until the boundary is implemented.

use unscroll_desktop_lib::{
    app_state::UnscrollState,
    commands::{
        self, ERROR_CODES, PROGRESS_EVENTS, SESSION_KINDS,
    },
    recovery::model::{BaselineInput, DeviceBinding, InitialPackageSuspension, RecoveryEnvelopeV1},
};

fn manifest_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn state() -> UnscrollState {
    UnscrollState::new()
}

#[test]
fn rejects_malformed_package_ids() {
    for bad in [
        "not a package!!",
        "single",
        ".leading.dot",
        "UPPER START.1bad",
        "",
    ] {
        let result = commands::validate_package_list(&[bad.to_owned()]);
        assert!(result.is_err(), "package accepted: {bad:?}");
        assert_eq!(result.unwrap_err().code, "invalid-package-id");
    }
}

#[test]
fn rejects_stale_device_ids() {
    let state = state();
    commands::device::bind_session(&state, "serial-a", "fp/fp-a").unwrap();
    let stale_serial = commands::device::check_session(&state, "serial-b", "fp/fp-a");
    assert_eq!(stale_serial.unwrap_err().code, "stale-device");
    let stale_fp = commands::device::check_session(&state, "serial-a", "fp/other");
    assert_eq!(stale_fp.unwrap_err().code, "stale-device");
}

#[test]
fn rejects_concurrent_mutations() {
    let state = state();
    commands::device::bind_session(&state, "serial-a", "fp/fp-a").unwrap();
    state.try_begin_mutation().unwrap();
    let second = state.try_begin_mutation();
    assert_eq!(second.unwrap_err().code, "busy-transaction");
    state.finish_mutation();
    assert!(state.try_begin_mutation().is_ok());
    state.finish_mutation();
}

#[test]
fn rejects_raw_command_shaped_payloads() {
    for bad in [
        "com.app; rm -rf /",
        "com.app && ls",
        "com.app | cat",
        "com.app $(evil)",
        "com.app `evil`",
        "com.app\nls",
        "-evil-flag",
    ] {
        let result = commands::reject_command_payload(bad);
        assert!(result.is_err(), "payload accepted: {bad:?}");
        let error = result.unwrap_err();
        assert_eq!(error.code, "rejected-command-payload");
        // Errors must never echo the raw payload back to the UI.
        assert!(!error.message.contains("rm -rf"));
        assert!(!error.message.contains("evil"));
    }
    // Caller-authored operation plans cannot pass as package ids either.
    let plan = r#"{"operation":"suspend","package":"com.app"}"#.to_owned();
    let result = commands::validate_package_list(&[plan]);
    assert!(result.is_err());
}

#[test]
fn rejects_invalid_confirmations() {
    assert_eq!(
        commands::recovery::validate_restore_confirmation("restore").unwrap_err().code,
        "invalid-confirmation"
    );
    assert_eq!(
        commands::maintenance::validate_open_confirmation("open").unwrap_err().code,
        "invalid-confirmation"
    );
    assert!(commands::recovery::validate_restore_confirmation("RESTORE MY PHONE").is_ok());
    assert!(commands::maintenance::validate_open_confirmation("OPEN STORE MAINTENANCE").is_ok());
}

#[test]
fn rejects_export_paths_without_user_selection() {
    assert_eq!(
        commands::diagnostics::validate_export_path("").unwrap_err().code,
        "export-path-required"
    );
    // A directory without a file name is not an explicit destination.
    assert_eq!(
        commands::diagnostics::validate_export_path("C:\\Users\\pc\\").unwrap_err().code,
        "export-path-required"
    );
    assert!(commands::diagnostics::validate_export_path("C:\\Users\\pc\\unscroll-diagnostics.json").is_ok());
}

#[test]
fn progress_event_ordering_matches_journal_ordering() {
    // Success: pending entries are applied in journal order, then complete.
    assert_eq!(
        commands::progress_sequence("success"),
        vec!["started", "operation-applied", "operation-applied", "completed"]
    );
    assert_eq!(
        commands::progress_sequence("pause"),
        vec!["started", "operation-applied", "decision-required"]
    );
    assert_eq!(
        commands::progress_sequence("resume"),
        vec!["decision-required", "operation-applied", "completed"]
    );
    assert_eq!(
        commands::progress_sequence("rollback"),
        vec!["started", "operation-applied", "rollback-started", "rollback-applied", "completed"]
    );
    assert_eq!(
        commands::progress_sequence("disconnect"),
        vec!["started", "operation-applied", "disconnected"]
    );
    assert_eq!(
        commands::progress_sequence("maintenance"),
        vec!["maintenance-opened", "maintenance-closed", "completed"]
    );
    assert_eq!(
        commands::progress_sequence("restore"),
        vec!["restore-started", "restore-completed"]
    );
}

#[test]
fn capability_files_grant_no_process_shell_or_arbitrary_fs() {
    let manifest = manifest_dir().join("capabilities/default.json");
    let text = std::fs::read_to_string(&manifest).expect("capability file must exist");
    // The schema reference is metadata, not a grant; permission checks below
    // are scoped to the declared permission entries only.
    assert!(text.contains("$schema"), "capability must reference its schema");
    let array = text
        .split_once("\"permissions\"")
        .expect("capability must declare permissions")
        .1;
    let array = array
        .split_once('[')
        .expect("permissions must be an array")
        .1
        .split_once(']')
        .expect("permissions array must close")
        .0;
    let entries: Vec<&str> = array
        .split('"')
        .map(str::trim)
        .filter(|part| !part.is_empty() && *part != "," && *part != ":")
        .collect();
    assert!(!entries.is_empty(), "capability must grant at least one entry");
    for entry in &entries {
        assert!(entry.starts_with("core:"), "non-core grant: {entry:?}");
        for forbidden in [
            "shell", "process", "cli", "adb", "fs", "http", "dialog", "notification", "updater",
            "global-shortcut", "os", "plugin",
        ] {
            assert!(
                !entry.contains(forbidden),
                "capability grants {forbidden:?} via {entry:?}"
            );
        }
    }
}

#[test]
fn rust_and_typescript_dto_fixtures_agree() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/lib/api/fixtures");
    let errors = std::fs::read_to_string(dir.join("error-codes.json")).expect("error fixture");
    let progress = std::fs::read_to_string(dir.join("progress-events.json")).expect("progress fixture");
    let kinds = std::fs::read_to_string(dir.join("session-kinds.json")).expect("session fixture");
    for code in ERROR_CODES {
        assert!(errors.contains(code), "fixture missing error code {code}");
    }
    for event in PROGRESS_EVENTS {
        assert!(progress.contains(event), "fixture missing progress event {event}");
    }
    for kind in SESSION_KINDS {
        assert!(kinds.contains(kind), "fixture missing session kind {kind}");
    }
}

#[test]
fn icons_are_bounded_and_released_on_new_inspection() {
    let state = state();
    let big = vec![0u8; 2 * 1024 * 1024];
    assert!(state.store_icon("com.app", big).is_err());
    state.store_icon("com.app", vec![1, 2, 3]).unwrap();
    assert_eq!(state.icon_count(), 1);
    // A new inspection drops the old catalog's icon memory.
    commands::device::note_inspection(&state, "serial-a", "fp/fp-a").unwrap();
    assert_eq!(state.icon_count(), 0);
}

#[test]
fn frontend_command_names_match_registered_handlers() {
    use unscroll_desktop_lib::commands::handlers::{invoked_command_names, COMMAND_NAMES};
    let root = manifest_dir();
    let invoke = std::fs::read_to_string(root.join("../src/lib/api/invoke.ts"))
        .expect("frontend transport must exist");
    let mut invoked = invoked_command_names(&invoke);
    invoked.sort();
    let mut expected: Vec<String> =
        COMMAND_NAMES.iter().map(|name| name.to_string()).collect();
    expected.sort();
    assert_eq!(invoked, expected, "invoke.ts drifts from COMMAND_NAMES");
    // Every contract name resolves to a real handler wired into lib.rs.
    let handlers = std::fs::read_to_string(root.join("src/commands/handlers.rs"))
        .expect("handlers module must exist");
    let lib = std::fs::read_to_string(root.join("src/lib.rs")).expect("lib.rs must exist");
    assert!(lib.contains("generate_handler"), "lib.rs must register handlers");
    for name in COMMAND_NAMES {
        assert!(handlers.contains(&format!("pub async fn {name}")), "missing handler {name}");
        assert!(lib.contains(name), "lib.rs does not register {name}");
    }
}

#[test]
fn progress_channel_matches_frontend_listener() {
    use unscroll_desktop_lib::commands::handlers::PROGRESS_CHANNEL;
    let events = std::fs::read_to_string(manifest_dir().join("../src/lib/api/events.ts"))
        .expect("frontend events must exist");
    assert_eq!(PROGRESS_CHANNEL, "unscroll-progress");
    assert!(
        events.contains("PROGRESS_EVENT = \"unscroll-progress\""),
        "frontend listens on a different channel"
    );
}

#[test]
fn rebind_while_busy_is_rejected() {
    let state = state();
    state.bind_device("serial-a", "fp/fp-a").unwrap();
    state.try_begin_mutation().unwrap();
    assert_eq!(state.bind_device("serial-b", "fp/fp-b").unwrap_err().code, "busy-transaction");
    assert_eq!(
        commands::device::note_inspection(&state, "serial-b", "fp/fp-b").unwrap_err().code,
        "busy-transaction"
    );
    state.finish_mutation();
    assert!(state.bind_device("serial-b", "fp/fp-b").is_ok());
}

#[test]
fn mutation_guard_releases_on_drop() {
    let state = state();
    {
        let _guard = state.begin_mutation().unwrap();
        assert!(state.is_busy());
        assert_eq!(state.try_begin_mutation().unwrap_err().code, "busy-transaction");
    }
    assert!(!state.is_busy());
    assert!(state.try_begin_mutation().is_ok());
    state.finish_mutation();
}

#[test]
fn overfull_icon_cache_has_distinct_code() {
    let state = state();
    for index in 0..unscroll_desktop_lib::app_state::MAX_ICONS {
        state.store_icon(&format!("com.app{index}"), vec![1]).unwrap();
    }
    assert_eq!(
        state.store_icon("com.overflow", vec![1]).unwrap_err().code,
        "icon-cache-full"
    );
    let big = vec![0u8; 2 * 1024 * 1024];
    assert_eq!(state.store_icon("com.big", big).unwrap_err().code, "icon-too-large");
}

#[test]
fn export_path_hardening_rejects_traversal_and_missing_parents() {
    use unscroll_desktop_lib::commands::diagnostics::canonicalize_export_path;
    assert_eq!(
        canonicalize_export_path("C:\\a\\..\\b\\x.json").unwrap_err().code,
        "rejected-command-payload"
    );
    assert_eq!(
        canonicalize_export_path("relative\\x.json").unwrap_err().code,
        "export-path-required"
    );
    assert_eq!(
        canonicalize_export_path("C:\\definitely\\missing\\dir-xyz\\x.json").unwrap_err().code,
        "export-path-required"
    );
    let ok = std::env::temp_dir().join("unscroll-export-probe.json");
    assert!(canonicalize_export_path(ok.to_str().unwrap()).is_ok());
    assert!(!ok.exists(), "validation must not create files");
}

fn baseline(serial: &str, fingerprint: &str) -> RecoveryEnvelopeV1 {
    RecoveryEnvelopeV1::new_baseline(BaselineInput {
        binding: DeviceBinding { serial: serial.into(), fingerprint: fingerprint.into(), user_id: 0 },
        baseline_id: "11111111-1111-1111-1111-111111111111".into(),
        baseline_launcher: "com.base".into(),
        initial_home: "com.base/.Home".into(),
        initial_packages: vec![InitialPackageSuspension {
            package: "com.app".into(),
            suspended: false,
            user_id: 0,
        }],
        allowed_packages: vec!["com.app".into()],
    })
    .unwrap()
}

#[test]
fn diagnostic_preview_bundle_is_redacted() {
    use unscroll_desktop_lib::commands::diagnostics::preview_bundle;
    let serial = "device-serial-123";
    let fingerprint = "maker/model/device:13/ABC/999:user/release-keys";
    let envelope = baseline(serial, fingerprint);
    let bundle = preview_bundle(&envelope, "Pixel", fingerprint);
    assert!(bundle.redacted_envelope.contains("REDACTED"));
    assert!(!bundle.redacted_envelope.contains(serial));
    assert!(!bundle.redacted_envelope.contains(fingerprint));
    assert_ne!(bundle.preview.fingerprint_redacted, fingerprint);
}

#[test]
fn dto_shapes_match_typescript() {
    use unscroll_desktop_lib::commands::{
        handlers::{APP_ENTRY_DTO_FIELDS, DIAGNOSTIC_PREVIEW_DTO_FIELDS, SESSION_DTO_FIELDS},
        recovery::SESSION_ACTIONS,
    };
    let types = std::fs::read_to_string(manifest_dir().join("../src/lib/api/types.ts"))
        .expect("frontend types must exist");
    for field in SESSION_DTO_FIELDS.iter()
        .chain(APP_ENTRY_DTO_FIELDS.iter())
        .chain(DIAGNOSTIC_PREVIEW_DTO_FIELDS.iter())
    {
        assert!(types.contains(field), "types.ts missing field {field}");
    }
    for action in SESSION_ACTIONS {
        assert!(types.contains(action), "types.ts missing action {action}");
    }
    for code in ERROR_CODES {
        assert!(types.contains(code), "types.ts missing error code {code}");
    }
    // Rust session mapping stays inside the contract vocabularies.
    for kind in SESSION_KINDS {
        assert!(types.contains(kind), "types.ts missing session kind {kind}");
    }
}

#[test]
fn error_dto_json_carries_code_and_escapes() {
    let error = commands::CommandError::new("rejected-command-payload", "a\"b\nc", "retry");
    let json = error.to_json();
    assert!(json.contains("\"code\":\"rejected-command-payload\""));
    assert!(json.contains("\\\"") && json.contains("\\n"));
    assert!(json.starts_with('{') && json.ends_with('}'));
}

#[test]
fn session_observation_from_mirrors() {
    use unscroll_desktop_lib::commands::recovery::{classify_mirrors, session_kind_name};
    let idle = classify_mirrors(None, None, "s", "f", false, true);
    assert_eq!(session_kind_name(idle.kind), "new-setup");
    assert!(idle.allowed_actions().contains(&unscroll_desktop_lib::transaction::SessionAction::BeginSetup));
    let traces = classify_mirrors(None, None, "s", "f", true, true);
    assert_eq!(session_kind_name(traces.kind), "blocked-inconsistency");
    let envelope = baseline("device-serial-123", "maker/model/device:13/ABC/999:user/release-keys");
    let canonical = envelope.canonical_json();
    let active = classify_mirrors(
        Some(&canonical),
        Some(&canonical),
        "device-serial-123",
        "maker/model/device:13/ABC/999:user/release-keys",
        false,
        true,
    );
    assert_eq!(session_kind_name(active.kind), "active-policy");
    // Unexplained device state fails closed even with valid mirrors.
    let blocked = classify_mirrors(Some(&canonical), Some(&canonical), "device-serial-123", "maker/model/device:13/ABC/999:user/release-keys", false, false);
    assert_eq!(session_kind_name(blocked.kind), "blocked-inconsistency");
}

#[test]
fn session_dto_json_uses_contract_names() {
    use unscroll_desktop_lib::commands::handlers::session_json;
    let session = unscroll_desktop_lib::commands::recovery::classify_mirrors(None, None, "s", "f", false, true);
    let json = session_json(&session);
    assert!(json.contains("\"kind\":\"new-setup\""));
    assert!(json.contains("\"allowedActions\":[\"begin-setup\"]"));
}
