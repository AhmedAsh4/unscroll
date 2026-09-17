// Single transport for every desktop/src call into Rust. Screens and state
// modules must import these helpers; direct `invoke` imports elsewhere are
// out of scope. Only validated strings and string arrays cross here — never
// device commands, operation plans, or shell fragments.
import { invoke } from "@tauri-apps/api/core";
import type {
  AppEntryDto,
  CommandError,
  DiagnosticPreviewDto,
  SessionDto,
} from "./types";

export type InvokeResult<T> = { ok: true; value: T } | { ok: false; error: CommandError };

async function call<T>(command: string, args?: Record<string, unknown>): Promise<InvokeResult<T>> {
  try {
    const value = await invoke<T>(command, args);
    return { ok: true, value };
  } catch (raw) {
    return { ok: false, error: raw as CommandError };
  }
}

export function discoverDevices(): Promise<InvokeResult<{ serial: string }>> {
  return call("discover_devices");
}

export function inspectDevice(serial: string): Promise<InvokeResult<AppEntryDto[]>> {
  return call("inspect_device", { serial });
}

export function getSession(serial: string, fingerprint: string): Promise<InvokeResult<SessionDto>> {
  return call("get_session", { serial, fingerprint });
}

export function startApply(
  serial: string,
  fingerprint: string,
  allowed: string[],
): Promise<InvokeResult<void>> {
  return call("start_apply", { serial, fingerprint, allowed });
}

export function respondToDecision(
  serial: string,
  fingerprint: string,
  decision: "continue" | "rollback" | "home-confirmed" | "home-cancelled",
): Promise<InvokeResult<void>> {
  return call("respond_to_decision", { serial, fingerprint, decision });
}

export function startEdit(
  serial: string,
  fingerprint: string,
  allowed: string[],
): Promise<InvokeResult<void>> {
  return call("start_edit", { serial, fingerprint, allowed });
}

export function openMaintenance(
  serial: string,
  fingerprint: string,
  confirmation: string,
): Promise<InvokeResult<void>> {
  return call("open_maintenance", { serial, fingerprint, confirmation });
}

export function closeMaintenance(
  serial: string,
  fingerprint: string,
  approved: string[],
  scanned: string[],
): Promise<InvokeResult<void>> {
  return call("close_maintenance", { serial, fingerprint, approved, scanned });
}

export function startRestore(
  serial: string,
  fingerprint: string,
  confirmation: string,
): Promise<InvokeResult<void>> {
  return call("start_restore", { serial, fingerprint, confirmation });
}

export function retryCleanup(serial: string, fingerprint: string): Promise<InvokeResult<void>> {
  return call("retry_cleanup", { serial, fingerprint });
}

export function previewDiagnostics(
  serial: string,
  fingerprint: string,
): Promise<InvokeResult<DiagnosticPreviewDto>> {
  return call("preview_diagnostics", { serial, fingerprint });
}

export function exportDiagnostics(
  serial: string,
  fingerprint: string,
  destination: string,
): Promise<InvokeResult<void>> {
  // Handoff (Tasks 18/19): the webview has no save-dialog permission, so the
  // destination must arrive as an explicit absolute path from a UI affordance
  // those tasks provide. Rust rejects anything else (relative, traversal,
  // missing parent) before touching the filesystem.
  return call("export_diagnostics", { serial, fingerprint, destination });
}

/**
 * Bounded per-icon read. Returns the cached icon as an `image/png` data
 * URL. A missing cache entry resolves to the typed miss
 * (`{"missing":true}`) — treat any non-`data:` payload (miss, empty, or
 * error) as the neutral fallback, never as a loud error.
 */
export function loadAppIcon(
  serial: string,
  fingerprint: string,
  packageId: string,
): Promise<InvokeResult<string>> {
  return call("load_app_icon", { serial, fingerprint, packageId });
}
