// Stable DTOs mirroring the Rust command boundary
// (desktop/src-tauri/src/commands/*). The webview never constructs device
// commands; it only passes validated strings through invoke.ts.

/** Every stable error code. Must match Rust ERROR_CODES and fixtures/error-codes.json. */
export type CommandErrorCode =
  | "invalid-package-id"
  | "invalid-serial"
  | "invalid-fingerprint"
  | "invalid-destination"
  | "stale-device"
  | "busy-transaction"
  | "invalid-confirmation"
  | "invalid-decision"
  | "export-path-required"
  | "rejected-command-payload"
  | "no-device"
  | "multiple-devices"
  | "unauthorized-device"
  | "device-unavailable"
  | "missing-driver"
  | "unsupported-device"
  | "preflight-failed"
  | "recovery-required"
  | "incompatible-launcher"
  | "icon-too-large"
  | "icon-cache-full"
  | "plan-rejected"
  | "maintenance-blocked"
  | "restore-blocked"
  | "export-failed";

/** Error DTO: stable code, human message, recovery action. */
export interface CommandError {
  code: CommandErrorCode;
  message: string;
  action: string;
}

/** Typed progress events. Must match Rust PROGRESS_EVENTS and fixtures/progress-events.json. */
export type ProgressEventName =
  | "started"
  | "operation-applied"
  | "decision-required"
  | "chooser-required"
  | "rollback-started"
  | "rollback-applied"
  | "completed"
  | "disconnected"
  | "maintenance-opened"
  | "maintenance-closed"
  | "restore-started"
  | "restore-completed"
  | "inconsistent-state";

/** Session kinds. Must match Rust SESSION_KINDS and fixtures/session-kinds.json. */
export type SessionKindName =
  | "new-setup"
  | "active-policy"
  | "maintenance-recovery"
  | "resumable-transaction"
  | "rollback-only"
  | "restore-ready"
  | "cleanup-retry"
  | "blocked-inconsistency";

/** Session actions offered for a session kind. */
export type SessionActionName =
  | "resume"
  | "rollback"
  | "restore"
  | "retry-cleanup"
  | "export-diagnostics"
  | "begin-setup";

export interface SessionDto {
  kind: SessionKindName;
  guidance: string;
  allowedActions: SessionActionName[];
}

/** Catalog entry shown in the chooser (icon loaded as bounded bytes via resources). */
export interface AppEntryDto {
  packageId: string;
  label: string;
  suspended: boolean;
  enabled: boolean;
  protected: boolean;
  protectedReason: string | null;
  iconCached: boolean;
}

/** Redacted diagnostic preview shown before the user picks an export destination. */
export interface DiagnosticPreviewDto {
  deviceModel: string;
  fingerprintRedacted: string;
  allowlistCount: number;
  initialPackageCount: number;
  operations: string[];
  errors: string[];
  warnings: string[];
  redactedEnvelope: string;
}

/** Terminal apply outcomes reported by start_apply / respond_to_decision. */
export type ApplyOutcomeName =
  | "complete"
  | "decision-required"
  | "chooser-required"
  | "rolled-back"
  | "recoverable-disconnect"
  | "inconsistent-state";

/** Terminal restore outcomes reported by start_restore. */
export type RestoreOutcomeName =
  | "complete"
  | "chooser-required"
  | "cleanup-retry"
  | "blocked"
  | "recoverable-disconnect";

export const RESTORE_CONFIRMATION = "RESTORE MY PHONE";
export const MAINTENANCE_CONFIRMATION = "OPEN STORE MAINTENANCE";
