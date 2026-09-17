//! Thin async Tauri handlers: the 12 frontend operations.
//!
//! Every handler follows one shape: validate UI strings with the typed
//! parsers -> gate on the inspected session binding (`stale-device`) ->
//! hold the mutation guard for device-mutating work (`busy-transaction`) ->
//! compose Task 7-14 services (`BundledAdb`, `inspect`, `initialize_adb`,
//! `AdbTransaction`, planner, runner, session, maintenance, restore,
//! diagnostics) -> map outcomes to [`CommandError`] JSON -> emit
//! [`ProgressEvent`] names on [`PROGRESS_CHANNEL`] in
//! [`progress_sequence`](super::progress_sequence) order.
//!
//! Handlers return `Result<String, String>` so no serialization dependency
//! crosses the boundary: success payloads are hand-built DTO JSON,
//! failures are [`CommandError::to_json`].
//!
//! Safety notes (invariants 1-2):
//! - No handler takes raw ADB arguments, shell strings, `DeviceOperation` /
//!   `AdbCommand` JSON, or caller-authored plans: no such parameter exists.
//!   The single internal `AdbCommand::Uninstall` (restore cleanup) uses only
//!   the validated session serial plus the constant launcher package id.
//! - `start_apply` never re-plans over an existing journal: the runner skips
//!   `Applied` ids by plan index, so a shorter re-plan would misalign ids
//!   and skip device work while the journal claims it applied. Retries and
//!   decisions reuse the stashed plan (`UnscrollState::plan_for`); a missing
//!   stash (e.g. after restart) fails closed with `plan-rejected`.
//! - `start_edit`, `open_maintenance`, and `close_maintenance` fail closed
//!   (`plan-rejected` / `maintenance-blocked`). Their services need live
//!   protection facts and rescan observations for already-active devices,
//!   which have no public supplier in Tasks 7-14: `inspect` refuses
//!   `RecoveryRequired` devices by design and the catalog drivers are locked
//!   inside it. Fabricating those sets would risk suspending a protected
//!   package, so the handlers validate, gate, read nothing mutable, and
//!   refuse. Follow-up: expose a bound-device facts re-read from `device/`
//!   (catalog + protected + stores for the bound serial/fingerprint); these
//!   three handlers are the only call sites waiting on it.
//! - Blocking ADB calls run directly: one mutation at a time plus 10-15s
//!   command timeouts bound the stall, and the guard releases on drop.

use std::path::PathBuf;

use tauri::{AppHandle, Emitter, Manager, State};

use crate::{
    adb::{require_success, Adb, AdbCommand, BundledAdb, Component, UserId},
    app_state::UnscrollState,
    device::{inspect, DeviceSnapshot, Discovery, LauncherArtifact},
    policy::{initial_apply, Plan},
    recovery::{
        mirror::MirrorStore,
        model::{sha256_hex, RecoveryEnvelopeV1},
    },
    transaction::{
        apply, AdbTransaction, ApplyDevice, ApplyError, DeviceFailure, RestoreDevice,
    },
};

use super::{
    device as device_edge, diagnostics as diagnostics_edge, maintenance as maintenance_edge,
    map_mirror_error, policy as policy_edge, recovery as recovery_edge, validate_fingerprint,
    validate_package_list, validate_serial, CommandError, ProgressEvent, CODE_DEVICE_UNAVAILABLE,
    CODE_PLAN_REJECTED, CODE_PREFLIGHT_FAILED, CODE_RECOVERY_REQUIRED,
};
use super::policy::{validate_apply_request, validate_decision};

/// Every command name the frontend may invoke. Must stay identical to the
/// `call("...")` strings in `desktop/src/lib/api/invoke.ts` and to the
/// `generate_handler!` list in `src/lib.rs` (asserted by test).
pub const COMMAND_NAMES: &[&str] = &[
    "discover_devices",
    "inspect_device",
    "get_session",
    "start_apply",
    "respond_to_decision",
    "start_edit",
    "open_maintenance",
    "close_maintenance",
    "start_restore",
    "retry_cleanup",
    "preview_diagnostics",
    "export_diagnostics",
];

/// Progress-event channel shared with `desktop/src/lib/api/events.ts`.
pub const PROGRESS_CHANNEL: &str = "unscroll-progress";

/// DTO field names, mirroring `desktop/src/lib/api/types.ts` (asserted).
pub const SESSION_DTO_FIELDS: &[&str] = &["kind", "guidance", "allowedActions"];
pub const APP_ENTRY_DTO_FIELDS: &[&str] = &[
    "packageId",
    "label",
    "suspended",
    "enabled",
    "protected",
    "protectedReason",
    "iconCached",
];
pub const DIAGNOSTIC_PREVIEW_DTO_FIELDS: &[&str] = &[
    "deviceModel",
    "fingerprintRedacted",
    "allowlistCount",
    "initialPackageCount",
    "operations",
    "errors",
    "warnings",
    "redactedEnvelope",
];
pub const APPLY_OUTCOME_NAMES: &[&str] = &[
    "complete",
    "decision-required",
    "chooser-required",
    "rolled-back",
    "recoverable-disconnect",
    "inconsistent-state",
];
pub const RESTORE_OUTCOME_NAMES: &[&str] = &[
    "complete",
    "chooser-required",
    "cleanup-retry",
    "blocked",
    "recoverable-disconnect",
];

const UNSCROLL_LAUNCHER: &str = "org.unscroll.launcher";
const UNSCROLL_HOME: &str = "org.unscroll.launcher/.MainActivity";

/// Extract `call("name")` command strings from the frontend transport for
/// the name-parity test.
pub fn invoked_command_names(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = source;
    while let Some(start) = rest.find("call(\"") {
        rest = &rest[start + 6..];
        let Some(end) = rest.find('"') else { break };
        out.push(rest[..end].to_owned());
        rest = &rest[end + 1..];
    }
    out
}

fn quote(value: &str) -> String {
    super::json_quote(value)
}

fn err_json(error: CommandError) -> String {
    error.to_json()
}

fn device_unavailable() -> CommandError {
    CommandError::new(
        CODE_DEVICE_UNAVAILABLE,
        "The phone stopped responding.",
        "Check the USB cable, then try again.",
    )
}

fn emit(app: &AppHandle, event: ProgressEvent) {
    let _ = app.emit(PROGRESS_CHANNEL, event.name());
}

fn emit_all(app: &AppHandle, events: &[&str]) {
    for name in events {
        let _ = app.emit(PROGRESS_CHANNEL, name);
    }
}

fn adb_for(app: &AppHandle) -> Result<BundledAdb, CommandError> {
    // Packaged resources first; the development tree fallback keeps
    // contributor runs working without changing production behavior (the
    // compiled-in dev path does not exist in an installed app).
    BundledAdb::for_app(app)
        .or_else(|_| BundledAdb::for_development())
        .map_err(|_| {
            CommandError::new(
                CODE_DEVICE_UNAVAILABLE,
                "The bundled ADB runtime is missing.",
                "Reinstall Unscroll, then reconnect the phone.",
            )
        })
}

fn staging_source() -> PathBuf {
    std::env::temp_dir().join("unscroll-shared-recovery.json")
}

fn device_txn<'a>(
    adb: &'a mut BundledAdb,
    serial: crate::adb::Serial,
    fingerprint: crate::adb::Fingerprint,
) -> AdbTransaction<'a, BundledAdb> {
    AdbTransaction::new(adb, serial, fingerprint, staging_source())
}

/// Locate the staged launcher APK plus its detached signing record. Both are
/// packaging outputs (Task 21 stages them under resources); a missing pair
/// fails closed before any device write. The record carries the expected
/// signing identity, which `inspect` verifies against the bridge-reported
/// signer — no placeholder identity is ever accepted here.
fn launcher_artifact(app: &AppHandle) -> Result<LauncherArtifact, CommandError> {
    let missing = || {
        CommandError::new(
            CODE_PREFLIGHT_FAILED,
            "The Unscroll launcher package is not staged with this installation.",
            "Reinstall Unscroll from a complete release, then try again.",
        )
    };
    let dir = app.path().resource_dir().map_err(|_| missing())?;
    for name in ["launcher/unscroll-launcher.apk", "launcher.apk"] {
        let apk = dir.join(name);
        if !apk.is_file() {
            continue;
        }
        let sidecar = apk.with_extension("apk.sha256");
        let sha = std::fs::read_to_string(&sidecar).map(|text| text.trim().to_owned());
        let Ok(sha) = sha else { return Err(missing()) };
        return LauncherArtifact::from_path(&apk, sha).map_err(|_| {
            CommandError::new(
                CODE_PREFLIGHT_FAILED,
                "The staged launcher package failed validation.",
                "Reinstall Unscroll from a complete release, then try again.",
            )
        });
    }
    Err(missing())
}

fn unscroll_home() -> Component {
    Component::parse(UNSCROLL_HOME).expect("constant home component")
}

fn baseline_id_for(serial: &str, fingerprint: &str) -> String {
    let hex = sha256_hex(format!("unscroll-v1:{serial}:{fingerprint}").as_bytes());
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32]
    )
}

/// Read the working envelope: the first parseable mirror copy. Both missing
/// means no session (`recovery-required`); present-but-unreadable means the
/// copies disagree or are corrupt (caller-supplied blocked code).
fn read_working_envelope(
    store: &mut impl MirrorStore,
    blocked_code: &'static str,
) -> Result<RecoveryEnvelopeV1, CommandError> {
    let private = store.read_private().map_err(|_| device_unavailable())?;
    let shared = store.read_shared().map_err(|_| device_unavailable())?;
    if private.is_none() && shared.is_none() {
        return Err(CommandError::new(
            CODE_RECOVERY_REQUIRED,
            "The phone holds no Unscroll recovery data.",
            "Inspect the phone first, or start a new setup.",
        ));
    }
    for text in private.iter().chain(shared.iter()) {
        if let Ok(envelope) = RecoveryEnvelopeV1::parse(text) {
            return Ok(envelope);
        }
    }
    Err(CommandError::new(
        blocked_code,
        "No readable recovery copy is on the phone.",
        "Reconnect and reconcile the session before retrying.",
    ))
}

fn map_apply_error(error: ApplyError) -> CommandError {
    match error {
        ApplyError::Mirror(error) => map_mirror_error(&error),
        ApplyError::Journal => CommandError::new(
            CODE_PLAN_REJECTED,
            "The transaction journal does not fit the plan.",
            "Reconnect and reconcile the session before retrying.",
        ),
    }
}

/// Restore/delete backend over the live adapter. Launcher removal targets
/// only the constant Unscroll package with the validated session serial;
/// nothing caller-supplied reaches the uninstall.
struct RestoreBackend<'a, A: Adb> {
    txn: AdbTransaction<'a, A>,
}

impl<A: Adb> MirrorStore for RestoreBackend<'_, A> {
    type Error = DeviceFailure;
    fn write_private(&mut self, value: &str) -> Result<(), DeviceFailure> {
        self.txn.write_private(value)
    }
    fn write_shared(&mut self, value: &str) -> Result<(), DeviceFailure> {
        self.txn.write_shared(value)
    }
    fn read_private(&mut self) -> Result<Option<String>, DeviceFailure> {
        self.txn.read_private()
    }
    fn read_shared(&mut self) -> Result<Option<String>, DeviceFailure> {
        self.txn.read_shared()
    }
    fn delete_private(&mut self) -> Result<(), DeviceFailure> {
        self.txn.delete_private()
    }
    fn delete_shared(&mut self) -> Result<(), DeviceFailure> {
        self.txn.delete_shared()
    }
}

impl<A: Adb> crate::transaction::ApplyDevice for RestoreBackend<'_, A> {
    fn mutate(&mut self, operation: &crate::policy::Operation) -> Result<(), DeviceFailure> {
        self.txn.mutate(operation)
    }
    fn verified(&mut self, operation: &crate::policy::Operation) -> Result<bool, DeviceFailure> {
        self.txn.verified(operation)
    }
    fn chooser(&mut self) -> Result<(), DeviceFailure> {
        self.txn.chooser()
    }
}

impl<A: Adb> RestoreDevice for RestoreBackend<'_, A> {
    fn uninstall_launcher(&mut self) -> Result<(), DeviceFailure> {
        let package = crate::adb::PackageId::parse(UNSCROLL_LAUNCHER)
            .map_err(|_| DeviceFailure::Inconsistent)?;
        let user = UserId::parse(0).map_err(|_| DeviceFailure::Inconsistent)?;
        let outcome = self
            .txn
            .adb
            .execute(AdbCommand::Uninstall {
                serial: self.txn.serial.clone(),
                user,
                package,
            })
            .map_err(|_| DeviceFailure::Disconnect)?;
        require_success(outcome).map(|_| ()).map_err(|_| DeviceFailure::Command)
    }
}

// --- DTO builders (pure, parity-tested) ------------------------------------

/// Session DTO JSON: kind, guidance, and the only actions proven safe.
pub fn session_json(session: &crate::transaction::Session) -> String {
    let actions = recovery_edge::allowed_action_names(session.kind)
        .iter()
        .map(|name| quote(name))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"kind\":{},\"guidance\":{},\"allowedActions\":[{}]}}",
        quote(recovery_edge::session_kind_name(session.kind)),
        quote(&session.guidance),
        actions
    )
}

fn protected_reason(snapshot: &DeviceSnapshot, package: &str) -> Option<String> {
    snapshot
        .protected
        .iter()
        .find(|fact| fact.package.as_str() == package)
        .map(|fact| fact.reason.clone())
}

/// Catalog DTO JSON: the chooser rows plus device identity. `cached` marks
/// which entries have a bounded icon in session state; the rest render the
/// neutral local fallback (never guessed brand art).
pub fn catalog_json(snapshot: &DeviceSnapshot, cached: &[bool]) -> String {
    let entries = snapshot
        .catalog
        .iter()
        .enumerate()
        .map(|(index, app)| {
            let reason = protected_reason(snapshot, app.package.as_str());
            let (protected, reason) = match reason {
                Some(reason) => ("true", quote(&reason)),
                None => ("false", "null".to_owned()),
            };
            format!(
                "{{\"packageId\":{},\"label\":{},\"suspended\":{},\"enabled\":{},\"protected\":{},\"protectedReason\":{},\"iconCached\":{}}}",
                quote(app.package.as_str()),
                quote(&app.label),
                app.suspended,
                app.enabled,
                protected,
                reason,
                cached.get(index).copied().unwrap_or(false)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"serial\":{},\"model\":{},\"manufacturer\":{},\"api\":{},\"entries\":[{}]}}",
        quote(&snapshot.serial),
        quote(&snapshot.model),
        quote(&snapshot.manufacturer),
        snapshot.api,
        entries
    )
}

/// Diagnostic preview DTO JSON: redacted preview fields plus the redacted
/// envelope for manual review. Never carries raw serials or fingerprints.
pub fn preview_json(
    bundle: &diagnostics_edge::PreviewBundle,
    model: &str,
) -> String {
    let list = |items: &[String]| items.iter().map(|item| quote(item)).collect::<Vec<_>>().join(",");
    format!(
        "{{\"deviceModel\":{},\"fingerprintRedacted\":{},\"allowlistCount\":{},\"initialPackageCount\":{},\"operations\":[{}],\"errors\":[{}],\"warnings\":[{}],\"redactedEnvelope\":{}}}",
        quote(model),
        quote(&bundle.preview.fingerprint_redacted),
        bundle.preview.allowlist_count,
        bundle.preview.initial_package_count,
        list(&bundle.preview.operations),
        list(&bundle.preview.errors),
        list(&bundle.preview.warnings),
        quote(&bundle.redacted_envelope)
    )
}

fn apply_outcome_name(outcome: &crate::transaction::ApplyOutcome) -> &'static str {
    match outcome {
        crate::transaction::ApplyOutcome::Complete => APPLY_OUTCOME_NAMES[0],
        crate::transaction::ApplyOutcome::DecisionRequired { .. } => APPLY_OUTCOME_NAMES[1],
        crate::transaction::ApplyOutcome::ChooserRequired => APPLY_OUTCOME_NAMES[2],
        crate::transaction::ApplyOutcome::RolledBack => APPLY_OUTCOME_NAMES[3],
        crate::transaction::ApplyOutcome::RecoverableDisconnect => APPLY_OUTCOME_NAMES[4],
        crate::transaction::ApplyOutcome::InconsistentState => APPLY_OUTCOME_NAMES[5],
    }
}

/// Apply outcome DTO JSON: terminal outcome, the blocking package for
/// decisions, and honestly-reported partial protection.
pub fn apply_outcome_json(
    outcome: &crate::transaction::ApplyOutcome,
    partial_protection: &[String],
) -> String {
    let package = match outcome {
        crate::transaction::ApplyOutcome::DecisionRequired { package } => quote(package.as_str()),
        _ => "null".to_owned(),
    };
    let partial = partial_protection.iter().map(|item| quote(item)).collect::<Vec<_>>().join(",");
    format!(
        "{{\"outcome\":{},\"package\":{},\"partialProtection\":[{}]}}",
        quote(apply_outcome_name(outcome)),
        package,
        partial
    )
}

fn restore_outcome_name(outcome: &crate::transaction::RestoreOutcome) -> &'static str {
    match outcome {
        crate::transaction::RestoreOutcome::Complete => RESTORE_OUTCOME_NAMES[0],
        crate::transaction::RestoreOutcome::ChooserRequired => RESTORE_OUTCOME_NAMES[1],
        crate::transaction::RestoreOutcome::CleanupRetry => RESTORE_OUTCOME_NAMES[2],
        crate::transaction::RestoreOutcome::Blocked => RESTORE_OUTCOME_NAMES[3],
        crate::transaction::RestoreOutcome::RecoverableDisconnect => RESTORE_OUTCOME_NAMES[4],
    }
}

/// Restore outcome DTO JSON.
pub fn restore_outcome_json(outcome: &crate::transaction::RestoreOutcome) -> String {
    format!("{{\"outcome\":{}}}", quote(restore_outcome_name(outcome)))
}

// --- Handlers ---------------------------------------------------------------

#[tauri::command]
pub async fn discover_devices(app: AppHandle) -> Result<String, String> {
    let mut adb = adb_for(&app).map_err(err_json)?;
    let discovery = device_edge::map_discovery_error;
    let found = crate::device::discover(&mut adb).map_err(|error| err_json(discovery(&error)))?;
    let Discovery::One { serial } = found;
    Ok(format!("{{\"serial\":{}}}", quote(serial.as_str())))
}

#[tauri::command]
pub async fn inspect_device(
    app: AppHandle,
    state: State<'_, UnscrollState>,
    serial: String,
) -> Result<String, String> {
    let serial = validate_serial(&serial).map_err(err_json)?;
    let _guard = state.begin_mutation().map_err(err_json)?;
    emit(&app, ProgressEvent::Started);
    let mut adb = adb_for(&app).map_err(err_json)?;
    let Discovery::One { serial: found } =
        crate::device::discover(&mut adb).map_err(|error| err_json(device_edge::map_discovery_error(&error)))?;
    if found != serial {
        return Err(err_json(CommandError::stale_device()));
    }
    let artifact = launcher_artifact(&app).map_err(err_json)?;
    let snapshot = inspect(&mut adb, Some(artifact))
        .map_err(|error| err_json(device_edge::map_inspection_error(&error)))?;
    state.bind_inspected(serial.as_str(), snapshot.fingerprint.as_str());
    state.clear_icons();
    let cached: Vec<bool> = snapshot
        .catalog
        .iter()
        .map(|app| state.store_icon(app.package.as_str(), app.icon.clone()).is_ok())
        .collect();
    emit(&app, ProgressEvent::Completed);
    Ok(catalog_json(&snapshot, &cached))
}

#[tauri::command]
pub async fn get_session(
    app: AppHandle,
    state: State<'_, UnscrollState>,
    serial: String,
    fingerprint: String,
) -> Result<String, String> {
    let (serial_typed, fingerprint_typed) =
        device_edge::validate_inspect_request(&serial, &fingerprint).map_err(err_json)?;
    device_edge::check_session(&state, &serial, &fingerprint).map_err(err_json)?;
    let mut adb = adb_for(&app).map_err(err_json)?;
    let serial_text = serial_typed.as_str().to_owned();
    let fingerprint_text = fingerprint_typed.as_str().to_owned();
    let mut txn = device_txn(&mut adb, serial_typed, fingerprint_typed);
    let private = txn.read_private().map_err(|_| err_json(device_unavailable()))?;
    let shared = txn.read_shared().map_err(|_| err_json(device_unavailable()))?;
    let session = match (&private, &shared) {
        (None, None) => {
            // No recovery data: only Unscroll HOME on the device counts as a
            // policy trace. A disconnect here is a transport failure, not an
            // empty session.
            let traces = match txn.verified(&crate::policy::Operation::Home {
                component: unscroll_home(),
            }) {
                Ok(hit) => hit,
                Err(DeviceFailure::Disconnect) => return Err(err_json(device_unavailable())),
                Err(_) => false,
            };
            recovery_edge::classify_mirrors(None, None, &serial_text, &fingerprint_text, traces, true)
        }
        _ => recovery_edge::classify_mirrors(
            private.as_deref(),
            shared.as_deref(),
            &serial_text,
            &fingerprint_text,
            false,
            true,
        ),
    };
    Ok(session_json(&session))
}

fn discard_on_terminal(state: &UnscrollState, serial: &str, outcome: &crate::transaction::ApplyOutcome) {
    use crate::transaction::ApplyOutcome as O;
    if matches!(outcome, O::Complete | O::RolledBack | O::InconsistentState) {
        state.discard_plan(serial);
    }
}

#[tauri::command]
pub async fn start_apply(
    app: AppHandle,
    state: State<'_, UnscrollState>,
    serial: String,
    fingerprint: String,
    allowed: Vec<String>,
) -> Result<String, String> {
    let allowed = validate_apply_request(&state, &serial, &fingerprint, &allowed).map_err(err_json)?;
    let _guard = state.begin_mutation().map_err(err_json)?;
    emit(&app, ProgressEvent::Started);
    let serial_text = serial.clone();
    let (serial_typed, fingerprint_typed) =
        device_edge::validate_inspect_request(&serial, &fingerprint).map_err(err_json)?;
    // Same-session retry reuses the stashed plan so journal ids stay aligned.
    if let Some(plan) = state.plan_for(&serial_text) {
        let mut adb = adb_for(&app).map_err(err_json)?;
        let mut txn = device_txn(&mut adb, serial_typed, fingerprint_typed);
        let envelope =
            read_working_envelope(&mut txn, CODE_PLAN_REJECTED).map_err(err_json)?;
        let result = apply(&mut txn, envelope, &plan, None).map_err(|error| err_json(map_apply_error(error)))?;
        discard_on_terminal(&state, &serial_text, &result.outcome);
        emit_all(&app, &policy_edge::apply_progress(&result.outcome));
        return Ok(apply_outcome_json(&result.outcome, &result.partial_protection));
    }
    let mut adb = adb_for(&app).map_err(err_json)?;
    let artifact = launcher_artifact(&app).map_err(err_json)?;
    let snapshot = inspect(&mut adb, Some(artifact))
        .map_err(|error| err_json(device_edge::map_inspection_error(&error)))?;
    if snapshot.serial != serial_text || snapshot.fingerprint != fingerprint {
        return Err(err_json(CommandError::stale_device()));
    }
    let plan: Plan = initial_apply(&snapshot, &allowed)
        .map_err(|error| err_json(policy_edge::map_plan_error(&error)))?;
    // Fresh setup only: existing history must resume via the stashed plan,
    // never via a shorter re-plan over applied ids.
    {
        let probe = device_txn(
            &mut adb,
            validate_serial(&serial_text).map_err(err_json)?,
            validate_fingerprint(&fingerprint).map_err(err_json)?,
        );
        let mut probe = probe;
        let existing = probe.read_private().map_err(|_| err_json(device_unavailable()))?;
        let existing_shared = probe.read_shared().map_err(|_| err_json(device_unavailable()))?;
        if existing.is_some() || existing_shared.is_some() {
            return Err(err_json(CommandError::new(
                CODE_PLAN_REJECTED,
                "Setup already started on this phone; it must resume, not restart.",
                "Answer the pending decision, or roll back before starting over.",
            )));
        }
    }
    let source = staging_source();
    let baseline_launcher = crate::adb::PackageId::parse(
        snapshot.home.as_str().split('/').next().unwrap_or_default(),
    )
    .map_err(|_| {
        err_json(CommandError::new(
            CODE_PLAN_REJECTED,
            "The current launcher cannot be recorded as the baseline.",
            "Reconnect and inspect the phone again.",
        ))
    })?;
    crate::recovery::mirror::initialize_adb(
        &mut adb,
        &snapshot,
        &baseline_id_for(&serial_text, &fingerprint),
        baseline_launcher,
        allowed.clone(),
        &source,
    )
    .map_err(|error| err_json(map_mirror_error(&error)))?;
    state.store_plan(&serial_text, plan.clone());
    let mut txn = device_txn(
        &mut adb,
        validate_serial(&serial_text).map_err(err_json)?,
        validate_fingerprint(&fingerprint).map_err(err_json)?,
    );
    let envelope = read_working_envelope(&mut txn, CODE_PLAN_REJECTED).map_err(err_json)?;
    let result = apply(&mut txn, envelope, &plan, None).map_err(|error| err_json(map_apply_error(error)))?;
    discard_on_terminal(&state, &serial_text, &result.outcome);
    emit_all(&app, &policy_edge::apply_progress(&result.outcome));
    Ok(apply_outcome_json(&result.outcome, &result.partial_protection))
}

#[tauri::command]
pub async fn respond_to_decision(
    app: AppHandle,
    state: State<'_, UnscrollState>,
    serial: String,
    fingerprint: String,
    decision: String,
) -> Result<String, String> {
    let decision = validate_decision(&decision).map_err(err_json)?;
    device_edge::check_session(&state, &serial, &fingerprint).map_err(err_json)?;
    let _guard = state.begin_mutation().map_err(err_json)?;
    emit(&app, ProgressEvent::Started);
    let serial_text = serial.clone();
    let (serial_typed, fingerprint_typed) =
        device_edge::validate_inspect_request(&serial, &fingerprint).map_err(err_json)?;
    let Some(plan) = state.plan_for(&serial_text) else {
        return Err(err_json(CommandError::new(
            CODE_PLAN_REJECTED,
            "No in-progress apply is known to this session.",
            "Start the apply from this session to answer its decisions.",
        )));
    };
    let mut adb = adb_for(&app).map_err(err_json)?;
    let mut txn = device_txn(&mut adb, serial_typed, fingerprint_typed);
    let envelope = read_working_envelope(&mut txn, CODE_PLAN_REJECTED).map_err(err_json)?;
    let result = apply(&mut txn, envelope, &plan, Some(decision))
        .map_err(|error| err_json(map_apply_error(error)))?;
    discard_on_terminal(&state, &serial_text, &result.outcome);
    emit_all(&app, &policy_edge::apply_progress(&result.outcome));
    Ok(apply_outcome_json(&result.outcome, &result.partial_protection))
}

#[tauri::command]
pub async fn start_edit(
    state: State<'_, UnscrollState>,
    serial: String,
    fingerprint: String,
    allowed: Vec<String>,
) -> Result<String, String> {
    let _allowed = validate_apply_request(&state, &serial, &fingerprint, &allowed).map_err(err_json)?;
    // Fail closed: edit must never suspend without live protection facts for
    // this bound device, and Tasks 7-14 expose no supplier for already-active
    // devices (`inspect` refuses `RecoveryRequired` devices by design; the
    // catalog drivers are locked inside it). Nothing is mutated or journaled
    // on this path. Follow-up: expose a bound-device facts re-read from
    // `device/`; this handler is its call site.
    Err(err_json(CommandError::new(
        CODE_PLAN_REJECTED,
        "Edit safety facts are unavailable for this phone right now.",
        "Reconnect the phone; if this persists, the desktop needs the facts update.",
    )))
}

#[tauri::command]
pub async fn open_maintenance(
    state: State<'_, UnscrollState>,
    serial: String,
    fingerprint: String,
    confirmation: String,
) -> Result<String, String> {
    maintenance_edge::validate_open_confirmation(&confirmation).map_err(err_json)?;
    device_edge::check_session(&state, &serial, &fingerprint).map_err(err_json)?;
    // Fail closed: opening restores recorded store state, and the recorded
    // store set is only provable with the same missing facts supplier as
    // `start_edit`. Nothing is mutated or journaled on this path.
    Err(err_json(CommandError::new(
        super::CODE_MAINTENANCE_BLOCKED,
        "Store safety facts are unavailable for this phone right now.",
        "Reconnect the phone; if this persists, the desktop needs the facts update.",
    )))
}

#[tauri::command]
pub async fn close_maintenance(
    state: State<'_, UnscrollState>,
    serial: String,
    fingerprint: String,
    approved: Vec<String>,
    scanned: Vec<String>,
) -> Result<String, String> {
    let _approved = validate_package_list(&approved).map_err(err_json)?;
    let _scanned = maintenance_edge::validate_scan(&scanned).map_err(err_json)?;
    device_edge::check_session(&state, &serial, &fingerprint).map_err(err_json)?;
    // Fail closed: closing must rescan launchable packages and store flags
    // live (same missing facts supplier as `start_edit`). A frontend-supplied
    // scan can omit installs, so it is validated but never trusted for the
    // rescan. Nothing is mutated or journaled on this path.
    Err(err_json(CommandError::new(
        super::CODE_MAINTENANCE_BLOCKED,
        "Store safety facts are unavailable for this phone right now.",
        "Reconnect the phone; if this persists, the desktop needs the facts update.",
    )))
}

#[tauri::command]
pub async fn start_restore(
    app: AppHandle,
    state: State<'_, UnscrollState>,
    serial: String,
    fingerprint: String,
    confirmation: String,
) -> Result<String, String> {
    recovery_edge::validate_restore_confirmation(&confirmation).map_err(err_json)?;
    device_edge::check_session(&state, &serial, &fingerprint).map_err(err_json)?;
    let _guard = state.begin_mutation().map_err(err_json)?;
    emit(&app, ProgressEvent::Started);
    let (serial_typed, fingerprint_typed) =
        device_edge::validate_inspect_request(&serial, &fingerprint).map_err(err_json)?;
    let mut adb = adb_for(&app).map_err(err_json)?;
    let txn = device_txn(&mut adb, serial_typed, fingerprint_typed);
    let mut backend = RestoreBackend { txn };
    let envelope =
        read_working_envelope(&mut backend, super::CODE_RESTORE_BLOCKED).map_err(err_json)?;
    let result = crate::transaction::restore(&mut backend, envelope, &confirmation)
        .map_err(|error| err_json(recovery_edge::map_restore_error(&error)))?;
    emit_all(&app, &recovery_edge::restore_progress(&result.outcome));
    Ok(restore_outcome_json(&result.outcome))
}

#[tauri::command]
pub async fn retry_cleanup(
    app: AppHandle,
    state: State<'_, UnscrollState>,
    serial: String,
    fingerprint: String,
) -> Result<String, String> {
    device_edge::check_session(&state, &serial, &fingerprint).map_err(err_json)?;
    let _guard = state.begin_mutation().map_err(err_json)?;
    emit(&app, ProgressEvent::Started);
    let (serial_typed, fingerprint_typed) =
        device_edge::validate_inspect_request(&serial, &fingerprint).map_err(err_json)?;
    let mut adb = adb_for(&app).map_err(err_json)?;
    let txn = device_txn(&mut adb, serial_typed, fingerprint_typed);
    let mut backend = RestoreBackend { txn };
    let envelope =
        read_working_envelope(&mut backend, super::CODE_RESTORE_BLOCKED).map_err(err_json)?;
    crate::transaction::retry_cleanup(&mut backend, envelope)
        .map_err(|error| err_json(recovery_edge::map_restore_error(&error)))?;
    emit(&app, ProgressEvent::Completed);
    Ok("{\"cleaned\":true}".to_owned())
}

#[tauri::command]
pub async fn preview_diagnostics(
    app: AppHandle,
    state: State<'_, UnscrollState>,
    serial: String,
    fingerprint: String,
) -> Result<String, String> {
    device_edge::check_session(&state, &serial, &fingerprint).map_err(err_json)?;
    let (serial_typed, fingerprint_typed) =
        device_edge::validate_inspect_request(&serial, &fingerprint).map_err(err_json)?;
    let fingerprint_text = fingerprint_typed.as_str().to_owned();
    let mut adb = adb_for(&app).map_err(err_json)?;
    let mut txn = device_txn(&mut adb, serial_typed, fingerprint_typed);
    let envelope =
        read_working_envelope(&mut txn, super::CODE_EXPORT_FAILED).map_err(err_json)?;
    let bundle = diagnostics_edge::preview_bundle(&envelope, "", &fingerprint_text);
    Ok(preview_json(&bundle, ""))
}

#[tauri::command]
pub async fn export_diagnostics(
    app: AppHandle,
    state: State<'_, UnscrollState>,
    serial: String,
    fingerprint: String,
    destination: String,
) -> Result<String, String> {
    let path = diagnostics_edge::canonicalize_export_path(&destination).map_err(err_json)?;
    device_edge::check_session(&state, &serial, &fingerprint).map_err(err_json)?;
    let _guard = state.begin_mutation().map_err(err_json)?;
    emit(&app, ProgressEvent::Started);
    let (serial_typed, fingerprint_typed) =
        device_edge::validate_inspect_request(&serial, &fingerprint).map_err(err_json)?;
    let fingerprint_text = fingerprint_typed.as_str().to_owned();
    let mut adb = adb_for(&app).map_err(err_json)?;
    let mut txn = device_txn(&mut adb, serial_typed, fingerprint_typed);
    let envelope =
        read_working_envelope(&mut txn, super::CODE_EXPORT_FAILED).map_err(err_json)?;
    let bundle = diagnostics_edge::preview_bundle(&envelope, "", &fingerprint_text);
    diagnostics_edge::write_diagnostic_export(&path, &bundle.redacted_envelope).map_err(err_json)?;
    emit(&app, ProgressEvent::Completed);
    Ok("{\"exported\":true}".to_owned())
}
