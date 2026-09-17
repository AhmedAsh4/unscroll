/**
 * Task 16: Guided Workspace connection-flow state.
 *
 * Framework-free and Tauri-free on purpose: this module only maps the stable
 * DTOs from `../api/types.ts` (plus `InvokeResult` shapes from
 * `../api/invoke.ts`) into presentation state. Screens inject the real
 * invoke.ts helpers; tests inject fakes. Nothing here constructs device
 * commands, shell strings, or operation plans, and nothing here renders raw
 * serials, fingerprints, commands, or exit codes — identity values stay in
 * memory for invoke arguments only.
 *
 * Known Task 15 edges handled defensively:
 * - Progress payloads may arrive as bare name strings or as objects; see
 *   `normalizeProgressPayload`.
 * - `start_edit` / `open_maintenance` / `close_maintenance` fail closed with
 *   `plan-rejected` / `maintenance-blocked`; those codes surface their
 *   server guidance (reconcile / close-first) and never retry silently.
 * - `inspect_device` may return the Rust object shape
 *   `{serial,model,manufacturer,api,entries}` or the legacy entries array;
 *   see `parseInspectResult`.
 */
import type { AppEntryDto, CommandError, CommandErrorCode, SessionDto, SessionKindName } from "../api/types.ts";
import type { InvokeResult } from "../api/invoke.ts";

export type { CommandError };

/** The four approved workflow steps, in order. */
export const STEPS = [
  { id: "connect", label: "Connect" },
  { id: "choose", label: "Choose apps" },
  { id: "review", label: "Review" },
  { id: "apply", label: "Apply" },
] as const;

export type StepId = (typeof STEPS)[number]["id"];
export type StepStatus = "complete" | "active" | "pending" | "blocked";

export interface StepState {
  id: StepId;
  label: string;
  status: StepStatus;
}

/**
 * Every connection condition the Connect flow can present. Operational
 * states (`idle`, `checking`, `inspecting`, `connected`, `ready`) track
 * progress; the rest map stable error codes (plus two derived warnings)
 * to human guidance with an adjacent recovery action.
 */
export type ConnectionState =
  | "idle"
  | "checking"
  | "inspecting"
  | "connected"
  | "ready"
  | "no-device"
  | "unauthorized-device"
  | "device-unavailable"
  | "multiple-devices"
  | "missing-driver"
  | "unsupported-device"
  | "unverified-newer"
  | "incompatible-launcher"
  | "recovery-required"
  | "maintenance-recovery"
  | "preflight-failed"
  | "stale-device"
  | "busy-transaction"
  | "device-replaced";

export type GuidanceTone = "info" | "warning" | "error";

export interface ConnectionGuidance {
  state: ConnectionState;
  tone: GuidanceTone;
  heading: string;
  body: string;
  /** Adjacent recovery action label rendered next to the message. */
  action: string;
}

/** Operation feedback becomes persistent after this delay (ms). */
export const SHOW_BUSY_AFTER_MS = 300;

/** Stable progress names shared with the Rust boundary. */
export const PROGRESS_NAMES = [
  "started",
  "operation-applied",
  "decision-required",
  "chooser-required",
  "rollback-started",
  "rollback-applied",
  "completed",
  "disconnected",
  "maintenance-opened",
  "maintenance-closed",
  "restore-started",
  "restore-completed",
  "inconsistent-state",
] as const;

export type ProgressName = (typeof PROGRESS_NAMES)[number];

const PROGRESS_SET: ReadonlySet<string> = new Set(PROGRESS_NAMES);

/** Extra Xiaomi/HyperOS bootstrap guidance appended to preflight failures. */
const MIUI_BODY =
  " On Xiaomi, Redmi, and HyperOS phones, turn on Install via USB in Developer options and accept the prompt on the phone. The manufacturer may require a signed-in Mi account, an inserted SIM card, or a network connection for that setting. This manufacturer requirement cannot be skipped from the desktop.";

const MAINTENANCE_CLOSE_NOTE =
  " Close maintenance before starting other actions; the phone stays fully usable until restrictions are re-applied.";

const RECOVERY_ROUTE_NOTE =
  " Reconnect and reconcile the session, then continue with edit, maintenance, or restore instead of a new setup.";

/** Static human copy for every connection state. No technical detail. */
export const CONNECTION_COPY: Record<ConnectionState, { heading: string; body: string; action: string }> = {
  idle: {
    heading: "Connect your phone",
    body: "Plug in one Android phone over USB with USB debugging enabled, then check for it.",
    action: "Check for phone",
  },
  checking: {
    heading: "Checking for a phone",
    body: "Looking for one USB-connected phone. This takes a moment.",
    action: "Wait",
  },
  inspecting: {
    heading: "Inspecting the phone",
    body: "Reading the phone model, Android version, and installed apps. Nothing on the phone is changed.",
    action: "Wait",
  },
  connected: {
    heading: "Phone found",
    body: "A phone answered over USB. Inspecting it now.",
    action: "Wait",
  },
  ready: {
    heading: "Phone ready",
    body: "The phone is connected and inspected. App selection is not available in this build yet.",
    action: "Continue",
  },
  "no-device": {
    heading: "No phone found",
    body: "Connect one phone over USB and enable USB debugging in Developer options, then check again.",
    action: "Check again",
  },
  "unauthorized-device": {
    heading: "Phone is waiting for authorization",
    body: "The phone is waiting for USB-debugging authorization. Accept the authorization prompt on the phone screen, then try again.",
    action: "Try again",
  },
  "device-unavailable": {
    heading: "The phone stopped responding",
    body: "Check the USB cable and port, keep the phone awake and unlocked, then try again.",
    action: "Try again",
  },
  "multiple-devices": {
    heading: "More than one phone is connected",
    body: "Leave exactly one phone connected over USB, unplug the others, then try again.",
    action: "Try again",
  },
  "missing-driver": {
    heading: "Windows cannot talk to the phone",
    body: "A Windows or manufacturer USB driver is missing. Follow the phone manufacturer's driver guidance, then reconnect the phone.",
    action: "Try again",
  },
  "unsupported-device": {
    heading: "This phone version is outside the tested range",
    body: "Unscroll supports capability-tested Android 7-16 phones. Use a capability-tested phone, or wait for a newer Unscroll release.",
    action: "Check again",
  },
  "unverified-newer": {
    heading: "This phone release is not verified yet",
    body: "This Android release is newer and unverified for Unscroll, so setup cannot continue. Use a capability-tested Android 7-16 phone, or wait for a newer Unscroll release.",
    action: "Check again",
  },
  "incompatible-launcher": {
    heading: "The launcher on the phone does not match",
    body: "The launcher on the phone is not compatible with this Unscroll release. Install the matching Unscroll release, then try again.",
    action: "Try again",
  },
  "recovery-required": {
    heading: "This phone already holds Unscroll data",
    body: "This phone already holds Unscroll recovery data. Reconnect and reconcile the session, then continue with edit, maintenance, or restore instead of a new setup.",
    action: "Reconcile session",
  },
  "maintenance-recovery": {
    heading: "A maintenance window is still open",
    body: "The phone records an open store maintenance window. Close maintenance before starting other actions; the phone stays fully usable until restrictions are re-applied.",
    action: "Reconcile session",
  },
  "preflight-failed": {
    heading: "The phone did not pass its checks",
    body: "The phone failed a preflight check and nothing was changed. Follow the on-screen guidance for the failed check and retry.",
    action: "Try again",
  },
  "stale-device": {
    heading: "This is not the inspected phone",
    body: "This phone is not the one that was inspected. Reconnect the inspected phone, or start a new inspection.",
    action: "Start a new inspection",
  },
  "busy-transaction": {
    heading: "Another phone operation is still running",
    body: "Another phone operation is still running. Wait for it to finish instead of unplugging the phone.",
    action: "Wait",
  },
  "device-replaced": {
    heading: "A different phone seems to be connected",
    body: "The connected phone does not match the inspected one. Reconnect the phone that was inspected, or start a new inspection.",
    action: "Start a new inspection",
  },
};

/** Detects payloads about releases newer than the verified range. */
export function isUnverifiedNewer(message: string): boolean {
  return /newer|unverified/i.test(message);
}

/** Detects Xiaomi/HyperOS bootstrap restriction signals. */
export function isMiuiGuidance(message: string): boolean {
  return /INSTALL_FAILED_USER_RESTRICTED|SecurityException|Install via USB/i.test(message);
}

/** Maps a stable error code to its connection state (message-agnostic). */
export function connectionFromCode(code: CommandErrorCode): ConnectionState {
  switch (code) {
    case "no-device":
      return "no-device";
    case "multiple-devices":
      return "multiple-devices";
    case "unauthorized-device":
      return "unauthorized-device";
    case "device-unavailable":
      return "device-unavailable";
    case "missing-driver":
      return "missing-driver";
    case "unsupported-device":
      return "unsupported-device";
    case "preflight-failed":
      return "preflight-failed";
    case "recovery-required":
      return "recovery-required";
    case "incompatible-launcher":
      return "incompatible-launcher";
    case "stale-device":
      return "stale-device";
    case "busy-transaction":
      return "busy-transaction";
    case "invalid-serial":
    case "invalid-fingerprint":
      return "device-replaced";
    case "plan-rejected":
    case "restore-blocked":
      // Fail-closed follow-ups: existing history must reconcile via
      // get_session instead of restarting, so route like recovery-required.
      return "recovery-required";
    case "maintenance-blocked":
      // Fail-closed follow-ups: an open maintenance window must close
      // before other actions, so route like maintenance-recovery.
      return "maintenance-recovery";
    default:
      return "preflight-failed";
  }
}

/**
 * Builds the full guidance for a command error: static copy plus the
 * server-provided message where it carries fail-closed detail, the
 * unverified-newer warning upgrade, and MIUI bootstrap guidance.
 */
export function guidanceForError(error: CommandError): ConnectionGuidance {
  let state = connectionFromCode(error.code);
  if ((state === "unsupported-device" || state === "preflight-failed") && isUnverifiedNewer(error.message)) {
    state = "unverified-newer";
  }
  const copy = CONNECTION_COPY[state];
  let body = copy.body;
  if (state === "preflight-failed") {
    const serverMessage = error.message.trim();
    if (serverMessage.length > 0 && !body.includes(serverMessage)) {
      body = `${body} Phone reported: ${serverMessage}`;
    }
    if (isMiuiGuidance(error.message)) {
      body += MIUI_BODY;
    }
  }
  if ((error.code === "plan-rejected" || error.code === "restore-blocked") && state === "recovery-required") {
    body = `${error.message}${RECOVERY_ROUTE_NOTE}`;
  }
  if (error.code === "maintenance-blocked" && state === "maintenance-recovery") {
    body = `${error.message}${MAINTENANCE_CLOSE_NOTE}`;
  }
  return {
    state,
    tone: state === "unverified-newer" ? "warning" : "error",
    heading: copy.heading,
    body,
    action: copy.action,
  };
}

/** Device facts parsed from an inspect payload. Identity stays in memory. */
export interface DeviceFacts {
  serial: string | null;
  model: string | null;
  manufacturer: string | null;
  api: number | null;
  fingerprint: string | null;
}

export interface InspectParsed {
  device: DeviceFacts;
  entries: AppEntryDto[];
}

const EMPTY_DEVICE: DeviceFacts = { serial: null, model: null, manufacturer: null, api: null, fingerprint: null };

function asText(value: unknown): string | null {
  return typeof value === "string" && value.length > 0 ? value : null;
}

function cleanEntry(value: unknown): AppEntryDto | null {
  if (typeof value !== "object" || value === null) return null;
  const row = value as Record<string, unknown>;
  if (typeof row["packageId"] !== "string" || typeof row["label"] !== "string") return null;
  return {
    packageId: row["packageId"],
    label: row["label"],
    suspended: row["suspended"] === true,
    enabled: row["enabled"] !== false,
    protected: row["protected"] === true,
    protectedReason: typeof row["protectedReason"] === "string" ? row["protectedReason"] : null,
    iconCached: row["iconCached"] === true,
  };
}

/**
 * Parses `inspect_device` results defensively: the Rust boundary returns
 * `{serial,model,manufacturer,api,entries}`, while older typings describe a
 * bare entries array. Malformed entries are dropped; garbage yields empty.
 */
export function parseInspectResult(value: unknown): InspectParsed {
  if (Array.isArray(value)) {
    return { device: { ...EMPTY_DEVICE }, entries: value.flatMap((item) => {
      const cleaned = cleanEntry(item);
      return cleaned ? [cleaned] : [];
    }) };
  }
  if (typeof value === "object" && value !== null) {
    const row = value as Record<string, unknown>;
    const rawEntries = Array.isArray(row["entries"]) ? row["entries"] : [];
    const api = typeof row["api"] === "number" && Number.isInteger(row["api"]) ? row["api"] : null;
    return {
      device: {
        serial: asText(row["serial"]),
        model: asText(row["model"]),
        manufacturer: asText(row["manufacturer"]),
        api,
        fingerprint: asText(row["fingerprint"]),
      },
      entries: rawEntries.flatMap((item) => {
        const cleaned = cleanEntry(item);
        return cleaned ? [cleaned] : [];
      }),
    };
  }
  return { device: { ...EMPTY_DEVICE }, entries: [] };
}

/**
 * Normalizes a progress payload that may be a bare name string (what Rust
 * currently emits on `unscroll-progress`) or an object payload, tolerating
 * one level of transport wrapping. Unknown payloads yield null, never throw.
 */
export function normalizeProgressPayload(payload: unknown): ProgressName | null {
  if (typeof payload === "string") {
    return PROGRESS_SET.has(payload) ? (payload as ProgressName) : null;
  }
  if (typeof payload === "object" && payload !== null) {
    const row = payload as Record<string, unknown>;
    if (typeof row["event"] === "string" && PROGRESS_SET.has(row["event"])) {
      return row["event"] as ProgressName;
    }
    if ("payload" in row) {
      const inner = row["payload"];
      if (inner !== payload) return normalizeProgressPayload(inner);
    }
  }
  return null;
}

/** Derives the four step states from the connection plus session routing. */
export function deriveSteps(connection: ConnectionState, session: SessionDto | null): StepState[] {
  const linked = connection === "ready" || connection === "recovery-required" || connection === "maintenance-recovery";
  const blocked = linked && (connection !== "ready" || (session !== null && session.kind !== "new-setup"));
  return STEPS.map((step, index): StepState => {
    if (!linked) {
      return { id: step.id, label: step.label, status: index === 0 ? "active" : "pending" };
    }
    if (index === 0) return { id: step.id, label: step.label, status: "complete" };
    if (blocked) return { id: step.id, label: step.label, status: "blocked" };
    if (index === 1) return { id: step.id, label: step.label, status: "active" };
    return { id: step.id, label: step.label, status: "pending" };
  });
}

export interface SessionRoute {
  title: string;
  body: string;
}

/** Human routing text for each reconciled session kind. */
export function routeForSession(kind: SessionKindName): SessionRoute {
  switch (kind) {
    case "new-setup":
      return {
        title: "Set up this phone",
        body: "No Unscroll policy was found on this phone. Continue to choose the apps to keep, then review and apply the policy.",
      };
    case "active-policy":
      return {
        title: "This phone already has a policy",
        body: "This phone already has an Unscroll policy. Reconcile the session, then edit allowed apps, open store maintenance, or restore the phone instead of starting a new setup.",
      };
    case "maintenance-recovery":
      return {
        title: "A maintenance window is still open",
        body: "A store maintenance window is still open from an interrupted session. It must close before other actions; closing re-applies store restrictions and verifies the policy.",
      };
    case "resumable-transaction":
      return {
        title: "An interrupted setup can resume",
        body: "An earlier setup stopped before finishing. Resume the remaining steps, or roll back the completed changes.",
      };
    case "rollback-only":
      return {
        title: "Only rollback is safe",
        body: "The recorded history only supports rollback. Roll back the completed changes, then start over.",
      };
    case "restore-ready":
      return {
        title: "Ready to restore",
        body: "The recorded changes can be restored. Review the recorded changes and type the confirmation to restore the phone.",
      };
    case "cleanup-retry":
      return {
        title: "Final cleanup needs one more step",
        body: "Everything is restored except final cleanup. Retry cleanup to remove the remaining recovery data.",
      };
    case "blocked-inconsistency":
      return {
        title: "Automatic changes are blocked",
        body: "The recovery records disagree, so no automatic change is safe. Review a diagnostic report before deciding the next step.",
      };
  }
}

export interface ConnectionDeps {
  discoverDevices: () => Promise<InvokeResult<{ serial: string }>>;
  inspectDevice: (serial: string) => Promise<InvokeResult<unknown>>;
  getSession: (serial: string, fingerprint: string) => Promise<InvokeResult<SessionDto>>;
}

/** Shell-level view update published by the Connect screen. */
export interface ShellUpdate {
  steps: StepState[];
  announcement: string;
  busy: boolean;
  statusState: "ok" | "busy" | "error" | "idle";
  statusText: string;
}

export interface WorkspaceSnapshot {
  connection: ConnectionState;
  guidance: ConnectionGuidance;
  device: DeviceFacts | null;
  entries: AppEntryDto[];
  session: SessionDto | null;
  busy: boolean;
}

function guidanceForState(state: ConnectionState): ConnectionGuidance {
  const copy = CONNECTION_COPY[state];
  return {
    state,
    tone: state === "unverified-newer" ? "warning" : state === "ready" || state === "idle" || state === "checking" || state === "inspecting" || state === "connected" ? "info" : "error",
    heading: copy.heading,
    body: copy.body,
    action: copy.action,
  };
}

/**
 * Connect-flow orchestrator: discover -> inspect -> reconcile the session.
 * Announces each connection change exactly once through the injected
 * `announce` callback (the shell renders it in its single polite region).
 * Concurrent `connect()` calls collapse into the running one.
 */
export class ConnectionFlow {
  snapshot: WorkspaceSnapshot;
  private readonly deps: ConnectionDeps;
  private readonly announce?: (message: string) => void;
  private readonly notify?: () => void;
  private lastAnnounced: string | null = null;

  constructor(deps: ConnectionDeps, opts?: { announce?: (message: string) => void; onChange?: () => void }) {
    this.deps = deps;
    this.announce = opts?.announce;
    this.notify = opts?.onChange;
    this.snapshot = {
      connection: "idle",
      guidance: guidanceForState("idle"),
      device: null,
      entries: [],
      session: null,
      busy: false,
    };
  }

  reset(): void {
    this.lastAnnounced = null;
    this.snapshot = {
      connection: "idle",
      guidance: guidanceForState("idle"),
      device: null,
      entries: [],
      session: null,
      busy: false,
    };
    this.notify?.();
  }

  /** Notes a remote progress event (bare string or object, per Task 15). */
  noteRemoteProgress(payload: unknown): void {
    const name = normalizeProgressPayload(payload);
    // Fail on disconnect in any non-idle connection: the cable-pull window
    // includes busy periods, so busy must not suppress the failure.
    if (name === "disconnected" && this.snapshot.connection !== "idle") {
      this.fail({ code: "device-unavailable", message: "progress reported a disconnect", action: "Check the USB cable, then try again." });
    }
  }

  async connect(): Promise<void> {
    if (this.snapshot.busy) return;
    this.patch({ busy: true });
    this.setState("checking");

    const discovered = await this.deps.discoverDevices();
    if (!discovered.ok) {
      this.fail(discovered.error);
      return;
    }
    const discoveredSerial = discovered.value.serial;
    this.setState("connected");
    this.setState("inspecting");

    const inspected = await this.deps.inspectDevice(discoveredSerial);
    if (!inspected.ok) {
      this.fail(inspected.error);
      return;
    }
    const parsed = parseInspectResult(inspected.value);
    if (parsed.device.serial !== null && parsed.device.serial !== discoveredSerial) {
      // Device-replacement guard: the inspected phone is not the found one.
      this.fail({
        code: "stale-device",
        message: "The inspected phone does not match the discovered phone.",
        action: "Reconnect the inspected phone, or start a new inspection.",
      });
      return;
    }
    this.patch({ device: parsed.device, entries: parsed.entries });

    if (parsed.device.fingerprint !== null) {
      const session = await this.deps.getSession(discoveredSerial, parsed.device.fingerprint);
      if (!session.ok) {
        this.fail(session.error);
        return;
      }
      this.patch({ session: session.value });
      if (session.value.kind === "maintenance-recovery") {
        this.patch({ busy: false });
        this.setState("maintenance-recovery");
        return;
      }
      // Task 16 decision (MINOR-5): only a new-setup session lands in
      // "ready". Any other reconciled kind already holds Unscroll data, so
      // it routes to "recovery-required" with the Reconcile action instead
      // of showing the "Phone ready" ok-pill plus a second route card.
      if (session.value.kind !== "new-setup") {
        this.patch({ busy: false });
        this.setState("recovery-required");
        return;
      }
    }
    this.patch({ busy: false });
    this.setState("ready");
  }

  private fail(error: CommandError): void {
    const guidance = guidanceForError(error);
    this.snapshot = { ...this.snapshot, connection: guidance.state, guidance, busy: false };
    this.say(`${guidance.heading} ${guidance.body}`);
    this.notify?.();
  }

  private setState(connection: ConnectionState): void {
    const guidance = guidanceForState(connection);
    this.snapshot = { ...this.snapshot, connection, guidance };
    this.say(`${guidance.heading} ${guidance.body}`);
    this.notify?.();
  }

  private patch(partial: Partial<WorkspaceSnapshot>): void {
    this.snapshot = { ...this.snapshot, ...partial };
    this.notify?.();
  }

  private say(message: string): void {
    if (message !== this.lastAnnounced) {
      this.lastAnnounced = message;
      this.announce?.(message);
    }
  }
}
