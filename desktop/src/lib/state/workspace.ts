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
 * - Edit and maintenance flows fail closed with
 *   `plan-rejected` / `maintenance-blocked`; those codes surface their
 *   server guidance (reconcile / close-first) and never retry silently.
 * - `inspect_device` may return the Rust object shape
 *   `{serial,model,manufacturer,api,entries}` or the legacy entries array;
 *   see `parseInspectResult`.
 */
import type { AppEntryDto, CommandError, CommandErrorCode, SessionDto, SessionKindName } from "../api/types.ts";
import type { InvokeResult } from "../api/invoke.ts";

export type { CommandError, SessionDto };

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
    body: "The phone is connected and inspected. Continue to choose the apps to keep.",
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
    isStore: row["isStore"] === true,
    isInstallSource: row["isInstallSource"] === true,
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

  /**
   * Restores a previously inspected snapshot (e.g. when the host remounts
   * the Connect screen after Back from the chooser) so the screen shows
   * the last inspection instead of re-running discovery. Never touches
   * the device; an explicit user recheck still runs a fresh `connect()`.
   */
  restoreSnapshot(snapshot: WorkspaceSnapshot): void {
    this.lastAnnounced = null;
    this.snapshot = { ...snapshot, busy: false };
    this.say(`${this.snapshot.guidance.heading} ${this.snapshot.guidance.body}`);
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

/* ------------------------------------------------------------------ */
/* Task 17: allowlist-first app selection, filtering, and review.      */
/*                                                                    */
/* Pure presentation logic only: everything here derives from already */
/* inspected `AppEntryDto` rows plus the read-only session DTO.       */
/* Nothing here calls into Rust, constructs device commands, or       */
/* mutates the phone. Screens use the discover/inspect/session         */
/* helpers from `../api/invoke.ts`; the chooser and review screens     */
/* never drive a transaction.                                         */
/*                                                                    */
/* Icon wiring point (honest backend reality): the inspect boundary   */
/* carries only the `iconCached` flag per entry; rows resolve bounded    */
/* PNG bytes lazily via `loadAppIcon` (`load_app_icon`: session-bound,   */
/* peek-never-removes, typed miss → fallback). `AppRow` therefore takes  */
/* an `iconSrc: string | null` prop: a resolved local data URL when the  */
/* loader provides one, otherwise null. A null (or failed) source        */
/* renders the neutral local fallback from `fallbackInitial` — the      */
/* app's initial letter plus its label. Never substitute remote art, a   */
/* brand catalog, or guessed icons.                                      */
/* ------------------------------------------------------------------ */

/** Chooser filter. Kept = kept incl. protected; Blocked = not kept. */
export type SelectionFilter = "all" | "kept" | "blocked";

/** Allowlist selection keyed by packageId (labels may repeat). */
export type SelectionMap = Record<string, boolean>;

/** Status pill states: chooser rows plus every review group. */
export type PillState = "kept" | "blocked" | "protected" | "store" | "unsupported";

/**
 * Store/sideload-source heuristic. The inspect boundary now carries
 * `isStore` / `isInstallSource` flags per entry (see `cleanEntry`), and
 * `reviewGroups` / `pillForEntry` prefer those flags whenever present.
 * This heuristic remains ONLY as a documented fallback for legacy shapes
 * without flags: the text is lowercased, split on non-alphanumeric
 * boundaries, and matched exactly against STORE_TOKENS.
 * Whole-token matching matters — substring matching mis-groups ordinary
 * apps such as "VLC Player" (play in player), "Display Tester" (play in
 * display), "SuperMarket List" (market in supermarket), or "Installment
 * Tracker" (install in installment). This stays informational grouping
 * only — never protection, never a mutation plan; real store restriction
 * stays a backend hard gate verified per package.
 */
export const STORE_TOKENS: ReadonlySet<string> = new Set([
  "store",
  "market",
  "play",
  "installer",
  "install",
  "sideload",
  "vending",
]);

/** Lowercase whole tokens of `packageId + label` for heuristic matching. */
export function storeTokensFor(entry: AppEntryDto): string[] {
  return `${entry.packageId} ${entry.label}`
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter((token) => token.length > 0);
}

/** True when the entry looks like a store or sideload source (see heuristic). */
export function isStoreLike(entry: AppEntryDto): boolean {
  return storeTokensFor(entry).some((token) => STORE_TOKENS.has(token));
}

/**
 * True when the DTO carries explicit store flags (the current Rust shape).
 * Legacy shapes without either flag fall back to `isStoreLike`.
 */
function hasStoreFlags(entry: AppEntryDto): boolean {
  const row = entry as unknown as Record<string, unknown>;
  return typeof row["isStore"] === "boolean" || typeof row["isInstallSource"] === "boolean";
}

/** True when a flagged entry is a store or install source. */
function isFlaggedStore(entry: AppEntryDto): boolean {
  return entry.isStore === true || entry.isInstallSource === true;
}

/** True for the stores review group: flags win, heuristic is legacy-only. */
function isStoreEntry(entry: AppEntryDto): boolean {
  if (hasStoreFlags(entry)) return isFlaggedStore(entry);
  return isStoreLike(entry);
}

/** Protected reasons that mark an entry as unsupported/uncertain. */
export const UNSUPPORTED_REASON = /unresolved|ambiguous|manufacturer|shared-role|uncertain|cannot/i;

/** Protected reason marking the hidden baseline launcher. */
export const BASELINE_LAUNCHER_REASON = /baseline launcher/i;

function isUnsupportedReason(reason: string | null): boolean {
  return reason !== null && UNSUPPORTED_REASON.test(reason);
}

function isBaselineLauncherReason(reason: string | null): boolean {
  return reason !== null && BASELINE_LAUNCHER_REASON.test(reason);
}

/**
 * Single shared pill mapping for chooser rows and review groups, derived
 * from the same reason predicates as `reviewGroups`: unsupported
 * protected entries read Unsupported in both screens, the baseline
 * launcher and other protected entries read Protected, and
 * non-protected entries read Kept, Store, or Blocked.
 */
export function pillForEntry(entry: AppEntryDto, kept: boolean): PillState {
  if (entry.protected) {
    if (!isBaselineLauncherReason(entry.protectedReason) && isUnsupportedReason(entry.protectedReason)) {
      return "unsupported";
    }
    return "protected";
  }
  if (kept) return "kept";
  if (isStoreEntry(entry)) return "store";
  return "blocked";
}

/**
 * Builds the allowlist-first selection for a fresh inspection: protected
 * entries stay kept and locked, everything safely blockable starts
 * blocked until the user keeps it. Call again for each new inspection —
 * stale packageIds are dropped and new packages get these defaults.
 */
export function createSelection(entries: AppEntryDto[]): SelectionMap {
  const selection: SelectionMap = {};
  for (const entry of entries) {
    selection[entry.packageId] = entry.protected;
  }
  return selection;
}

/**
 * Stable identity for an inspection result. PackageIds are sorted so a
 * mere catalog reorder never reseeds: the host keeps the selection
 * created for the last key and only calls `createSelection` again when
 * the key changes — so Choose <-> Review back-navigation retains the
 * user's toggles while a genuinely new inspection resets them.
 */
export function selectionKeyFor(entries: AppEntryDto[]): string {
  return entries
    .map((entry) => entry.packageId)
    .sort()
    .join("\n");
}

/**
 * Flips one non-protected entry. Protected toggles are a no-op (the
 * returned map keeps the protected entry kept). Never mutates `selection`.
 */
export function toggleSelection(selection: SelectionMap, entry: AppEntryDto): SelectionMap {
  const next: SelectionMap = { ...selection };
  if (!entry.protected) {
    next[entry.packageId] = !(selection[entry.packageId] === true);
  }
  return next;
}

export interface SelectionCounts {
  kept: number;
  blocked: number;
  total: number;
}

/** Counts kept (selected, incl. protected), blocked, and total entries. */
export function selectionCounts(selection: SelectionMap, entries: AppEntryDto[]): SelectionCounts {
  let kept = 0;
  for (const entry of entries) {
    if (selection[entry.packageId] === true) kept += 1;
  }
  return { kept, blocked: entries.length - kept, total: entries.length };
}

/**
 * Pure catalog view: case-insensitive substring search over label AND
 * packageId (surrounding whitespace trimmed), combined with the
 * All/Kept/Blocked filter. Never mutates (or reads beyond) the
 * selection map.
 */
export function filterApps(
  entries: AppEntryDto[],
  selection: SelectionMap,
  query: string,
  filter: SelectionFilter,
): AppEntryDto[] {
  const q = query.trim().toLowerCase();
  return entries.filter((entry) => {
    const kept = selection[entry.packageId] === true;
    if (filter === "kept" && !kept) return false;
    if (filter === "blocked" && kept) return false;
    if (q.length === 0) return true;
    return entry.label.toLowerCase().includes(q) || entry.packageId.toLowerCase().includes(q);
  });
}

export interface ReviewGroups {
  /** Selected, non-protected apps that will remain available. */
  kept: AppEntryDto[];
  /** Deselected, non-protected, non-store apps that will be suspended and hidden. */
  blocked: AppEntryDto[];
  /** Deselected, non-protected store-like apps Unscroll will restrict. */
  stores: AppEntryDto[];
  /** Protected entries that stay available (includes the baseline launcher). */
  protected: AppEntryDto[];
  /** Protected entries whose reason marks them unsupported/uncertain. */
  unsupported: AppEntryDto[];
  /** The baseline launcher entry when present (also listed in `protected`). */
  baselineLauncher: AppEntryDto | null;
}

/**
 * Partitions the catalog into the five review groups. Every entry lands
 * in exactly one of kept/blocked/stores/protected/unsupported, so group
 * sizes always reconcile to the catalog size.
 */
export function reviewGroups(entries: AppEntryDto[], selection: SelectionMap): ReviewGroups {
  const groups: ReviewGroups = {
    kept: [],
    blocked: [],
    stores: [],
    protected: [],
    unsupported: [],
    baselineLauncher: null,
  };
  for (const entry of entries) {
    if (entry.protected) {
      if (isBaselineLauncherReason(entry.protectedReason)) {
        if (groups.baselineLauncher === null) groups.baselineLauncher = entry;
        groups.protected.push(entry);
      } else if (isUnsupportedReason(entry.protectedReason)) {
        groups.unsupported.push(entry);
      } else {
        groups.protected.push(entry);
      }
      continue;
    }
    if (selection[entry.packageId] === true) {
      groups.kept.push(entry);
    } else if (isStoreEntry(entry)) {
      groups.stores.push(entry);
    } else {
      groups.blocked.push(entry);
    }
  }
  return groups;
}

export interface ApplyGate {
  connection: ConnectionState;
  session: SessionDto | null;
  entries: AppEntryDto[];
}

/** Apply is enabled only for a ready, new-setup inspection with apps. */
export function canApply(gate: ApplyGate): boolean {
  return gate.connection === "ready" && gate.session?.kind === "new-setup" && gate.entries.length > 0;
}

/**
 * Plain-text reason a disabled Apply is disabled (null when allowed).
 * Rendered as text next to the button — never color alone.
 */
export function applyBlockReason(
  connection: ConnectionState,
  session: SessionDto | null,
  entries: AppEntryDto[],
): string | null {
  if (entries.length === 0) return "No apps were found on the phone, so there is nothing to apply.";
  if (connection !== "ready") return "Connect and inspect the phone before applying.";
  if (session?.kind !== "new-setup") {
    return "This phone already holds Unscroll data. Reconcile the session instead of starting a new setup.";
  }
  return null;
}

/** Text count summary, e.g. "3 kept · 12 blocked · 15 total". */
export function countText(kept: number, blocked: number, total: number): string {
  return `${kept} kept · ${blocked} blocked · ${total} total`;
}

/**
 * Neutral icon fallback: the first letter of the app label ("?" when the
 * label is empty). Local text only — never brand art.
 */
export function fallbackInitial(label: string): string {
  const first = label.trim().charAt(0);
  return first === "" ? "?" : first.toUpperCase();
}

/* ------------------------------------------------------------------ */
/* Bounded lazy icons (boundary-hardening pass).                        */
/*                                                                      */
/* The inspect boundary carries only the `iconCached` flag per entry;   */
/* rows resolve the bytes lazily via `loadAppIcon` in `../api/invoke.ts`*/
/* (Rust `load_app_icon`: session-bound, bounded, peek-never-removes).  */
/* A missing entry is a typed miss (`{"missing":true}`) — every miss,   */
/* empty payload, garbage string, or loader error resolves to null so   */
/* rows render the neutral `fallbackInitial` fallback. Never substitute */
/* remote art, a brand catalog, or guessed icons.                       */
/* ------------------------------------------------------------------ */

const ICON_DATA_PREFIX = "data:image/png;base64,";

/**
 * Maps one icon transport value to a renderable source or null.
 * Only `image/png` data URLs with strict standard-alphabet base64
 * (correct length and padding) pass; the Rust typed miss, empty strings,
 * foreign schemes (`javascript:`, `data:image/svg+xml`, …), malformed
 * payloads, and non-strings all fall back to null (neutral fallback)
 * instead of reaching the `<img>` element.
 */
export function resolveIconSrc(value: unknown): string | null {
  if (typeof value !== "string") return null;
  if (!value.startsWith(ICON_DATA_PREFIX) || value.length <= ICON_DATA_PREFIX.length) return null;
  const body = value.slice(ICON_DATA_PREFIX.length);
  if (body.length === 0 || body.length % 4 !== 0) return null;
  if (!/^[A-Za-z0-9+/]*={0,2}$/.test(body)) return null;
  const padStart = body.indexOf("=");
  if (padStart !== -1 && !/^={1,2}$/.test(body.slice(padStart))) return null;
  return value;
}

/** Underlying transport for one package: data URL, miss marker, or null. */
export type IconTransport = (packageId: string) => Promise<unknown>;

export interface IconLoader {
  /** Resolves to a data URL or null (miss/error → null fallback). */
  get(packageId: string): Promise<string | null>;
  /** Synchronous cache probe: data URL, null (known miss), or undefined. */
  cached(packageId: string): string | null | undefined;
}

/**
 * Lazy per-package icon cache with in-flight dedup: concurrent `get`
 * calls for one packageId share a single transport flight, successes and
 * misses alike are cached so rows fire once per packageId, and every
 * failure resolves to null (neutral fallback, never a loud error).
 */
export function createIconLoader(load: IconTransport): IconLoader {
  const cache = new Map<string, string | null>();
  const inflight = new Map<string, Promise<string | null>>();
  return {
    get(packageId: string): Promise<string | null> {
      const hit = cache.get(packageId);
      if (hit !== undefined) return Promise.resolve(hit);
      const running = inflight.get(packageId);
      if (running !== undefined) return running;
      const flight = Promise.resolve()
        .then(() => load(packageId))
        .then(
          (value) => resolveIconSrc(value),
          () => null,
        )
        .then((resolved) => {
          cache.set(packageId, resolved);
          inflight.delete(packageId);
          return resolved;
        });
      inflight.set(packageId, flight);
      return flight;
    },
    cached(packageId: string): string | null | undefined {
      return cache.get(packageId);
    },
  };
}

/** Row-level icon decision: a renderable source or a single load request. */
export type RowIconDecision = { readonly kind: "value"; readonly src: string | null } | { readonly kind: "load" };

/**
 * Resolves one row's icon with fresh-catalog precedence. When the current
 * entry says `iconCached === false`, the answer is null even if the loader
 * still holds a stale hit: Rust clears the icon cache on every inspection,
 * so a same-packageId entry may go uncached while an old hit lingers.
 * Loader hits/misses are honored only while the catalog still says
 * cached; unknown entries request exactly one load.
 */
export function selectRowIcon(
  entry: AppEntryDto,
  cached: string | null | undefined,
): RowIconDecision {
  if (!entry.iconCached) return { kind: "value", src: null };
  if (cached !== undefined) return { kind: "value", src: cached };
  return { kind: "load" };
}

/** Inspected data handed from Connect to the chooser (identity stays out). */
export interface InspectedInfo {
  entries: AppEntryDto[];
  session: SessionDto | null;
  connection: ConnectionState;
}

/* ------------------------------------------------------------------ */
/* Task 18: guided apply state machine (presentation only).             */
/*                                                                     */
/* Pure mapping over the Task 15 boundary: the backend owns every      */
/* mutation and reports terminal outcomes as JSON text                 */
/* `{outcome,package,partialProtection}` from the apply-start and      */
/* decision-response calls. This machine never constructs device       */
/* commands or plans; it re-checks the live device before any call,    */
/* tracks ordered progress, and completes only on the backend          */
/* `complete` outcome. Identity values stay in memory for invoke       */
/* arguments only. Progress payloads arrive as JSON text and are       */
/* decoded here the same way `decodeProgressPayload` does, then        */
/* narrowed with `normalizeProgressPayload`.                           */
/* ------------------------------------------------------------------ */

/** Terminal apply outcomes reported by the backend envelope. */
export const APPLY_OUTCOMES = [
  "complete",
  "decision-required",
  "chooser-required",
  "rolled-back",
  "recoverable-disconnect",
  "inconsistent-state",
] as const;

export type ApplyOutcomeName = (typeof APPLY_OUTCOMES)[number];

const APPLY_OUTCOME_SET: ReadonlySet<string> = new Set(APPLY_OUTCOMES);

/** Domain answers the apply UI may send (never invented elsewhere). */
export type ApplyDecisionName = "continue" | "rollback" | "home-confirmed" | "home-cancelled";

/** Parsed backend outcome envelope: outcome, blocking package, partial gaps. */
export interface ApplyOutcomeDto {
  outcome: ApplyOutcomeName;
  package: string | null;
  partialProtection: string[];
}

/**
 * Parses one apply-start / decision-response value. Accepts the Rust
 * JSON-text envelope or an already-structured object; anything else (bare
 * names, garbage, unknown outcomes, mistyped fields) yields null, never
 * throws.
 */
export function parseApplyOutcome(value: unknown): ApplyOutcomeDto | null {
  let raw: unknown = value;
  if (typeof raw === "string") {
    const trimmed = raw.trim();
    if (!trimmed.startsWith("{")) return null;
    try {
      raw = JSON.parse(trimmed) as unknown;
    } catch {
      return null;
    }
  }
  if (typeof raw !== "object" || raw === null) return null;
  const row = raw as Record<string, unknown>;
  if (typeof row["outcome"] !== "string" || !APPLY_OUTCOME_SET.has(row["outcome"])) return null;
  const pkg = row["package"];
  if (pkg !== null && (typeof pkg !== "string" || pkg.length === 0)) return null;
  const partial = row["partialProtection"];
  if (!Array.isArray(partial) || partial.some((item) => typeof item !== "string")) return null;
  return {
    outcome: row["outcome"] as ApplyOutcomeName,
    package: (pkg as string | null) ?? null,
    partialProtection: [...(partial as string[])],
  };
}

/** Allowlist for the apply call: kept packageIds (selection true), sorted. */
export function allowlistFor(entries: AppEntryDto[], selection: SelectionMap): string[] {
  const kept: string[] = [];
  for (const item of entries) {
    if (selection[item.packageId] === true) kept.push(item.packageId);
  }
  return kept.sort();
}

/** Resolves a blocking packageId to its label for the decision screen. */
export function blockingAppFor(
  entries: AppEntryDto[],
  packageId: string | null,
): { packageId: string; label: string } | null {
  if (packageId === null || packageId.length === 0) return null;
  const found = entries.find((item) => item.packageId === packageId);
  return found ? { packageId: found.packageId, label: found.label } : null;
}

/** Decodes one JSON-text progress payload without throwing (never a throw). */
function decodeJsonText(raw: unknown): unknown {
  if (typeof raw !== "string") return raw;
  const trimmed = raw.trim();
  if (!trimmed.startsWith("{")) return raw;
  try {
    return JSON.parse(trimmed) as unknown;
  } catch {
    return raw;
  }
}

/**
 * Extracts the progress detail (the blocking packageId for
 * `decision-required`) from a raw payload. Tolerates JSON text, objects,
 * and one transport wrapping level; anything else yields null.
 */
export function extractProgressDetail(payload: unknown): string | null {
  const raw = decodeJsonText(payload);
  if (typeof raw === "object" && raw !== null) {
    const row = raw as Record<string, unknown>;
    if (typeof row["detail"] === "string" && row["detail"].length > 0) return row["detail"];
    if ("payload" in row && row["payload"] !== raw) return extractProgressDetail(row["payload"]);
  }
  return null;
}

/** Apply lifecycle. Terminal states never restart from the same flow. */
export type ApplyStatus =
  | "idle"
  | "verifying"
  | "running"
  | "decision-required"
  | "chooser-required"
  | "rolling-back"
  | "complete"
  | "rolled-back"
  | "recoverable-disconnect"
  | "inconsistent-state"
  | "failed";

export interface ApplySnapshot {
  status: ApplyStatus;
  busy: boolean;
  announcement: string;
  appliedCount: number;
  rollbackCount: number;
  blockingPackage: string | null;
  partialProtection: string[];
  error: CommandError | null;
  outcome: ApplyOutcomeName | null;
  /**
   * Origin of a `rolled-back` terminal state. `direct` means the backend
   * reported `rolled-back` without a preceding user rollback answer in this
   * flow (the store hard-failure path, the only case with a known store
   * cause). `user-rollback` / `chooser-cancel` mean the rollback followed
   * an explicit user answer. Null unless the status is `rolled-back`.
   */
  rolledBackFrom: "user-rollback" | "chooser-cancel" | "direct" | null;
}

/**
 * Injected transport. Values stay `unknown` because the Rust boundary
 * returns the outcome envelope as JSON text while older typings describe
 * narrower shapes; `parseApplyOutcome` narrows before anything is trusted.
 */
export type ApplyInvokeResult = { ok: true; value: unknown } | { ok: false; error: CommandError };

export interface ApplyDeps {
  discoverDevices: () => Promise<ApplyInvokeResult>;
  startApply: (serial: string, fingerprint: string, allowed: string[]) => Promise<ApplyInvokeResult>;
  respondToDecision: (serial: string, fingerprint: string, decision: ApplyDecisionName) => Promise<ApplyInvokeResult>;
}

export interface ApplyInput {
  serial: string;
  fingerprint: string;
  allowed: string[];
  entries: AppEntryDto[];
}

/** True only for the backend-verified complete outcome, never progress alone. */
export function isApplyComplete(snapshot: Pick<ApplySnapshot, "status" | "outcome">): boolean {
  return snapshot.status === "complete" && snapshot.outcome === "complete";
}

/**
 * @deprecated Do not use for copy branching. The backend does NOT guarantee
 * a direct `rolled-back` is a store failure — transaction/runner.rs yields
 * direct RolledBack for LauncherPolicy failure, Verify failure, and
 * prior-session resume too, and the envelope carries no cause. Kept only as
 * an origin signal awaiting a future backend `reason` field.
 */
export function isStoreRollback(
  snapshot: Pick<ApplySnapshot, "status" | "rolledBackFrom">,
): boolean {
  return snapshot.status === "rolled-back" && snapshot.rolledBackFrom === "direct";
}

/** Generic rolled-back recovery (cause unknown / ordinary user rollback). */
export const ROLLED_BACK_GENERIC_RECOVERY =
  "All completed changes were rolled back and verified; the phone is unchanged. Review the selection and try again.";

/** Store hard-failure recovery (direct backend rolled-back only). */
export const ROLLED_BACK_STORE_RECOVERY =
  "All completed changes were rolled back and verified; the phone is unchanged. Setup cannot continue with incomplete store protection: every detected store must be restricted for setup to finish. Review the selection and try again.";

/** Generic rolled-back lede (cause unknown / ordinary user rollback). */
export const ROLLED_BACK_GENERIC_LEDE =
  "All completed changes were rolled back and verified; the phone is unchanged. Review the selection and try again.";

/** Store hard-failure lede (direct backend rolled-back only). */
export const ROLLED_BACK_STORE_LEDE =
  "All completed changes were rolled back and verified, so the phone is unchanged. Setup cannot continue with incomplete store protection: every detected store must be restricted for setup to finish.";

/**
 * Rolled-back recovery copy: always the generic sentence. The backend
 * envelope carries no rollback cause (direct `rolled-back` also occurs for
 * LauncherPolicy failure, Verify failure, and prior-session resume), so no
 * branch may attribute it to stores. Null unless the status is `rolled-back`.
 */
export function rolledBackRecoveryText(
  snapshot: Pick<ApplySnapshot, "status" | "rolledBackFrom">,
): string | null {
  if (snapshot.status !== "rolled-back") return null;
  return ROLLED_BACK_GENERIC_RECOVERY;
}

/**
 * Rolled-back lede copy: always the generic sentence for the same
 * no-cause reason as `rolledBackRecoveryText`. Null unless `rolled-back`.
 */
export function rolledBackLedeText(
  snapshot: Pick<ApplySnapshot, "status" | "rolledBackFrom">,
): string | null {
  if (snapshot.status !== "rolled-back") return null;
  return ROLLED_BACK_GENERIC_LEDE;
}

function isApplyTerminal(status: ApplyStatus): boolean {
  return (
    status === "complete" ||
    status === "rolled-back" ||
    status === "recoverable-disconnect" ||
    status === "inconsistent-state"
  );
}

/**
 * Apply orchestrator: live device re-check, then exactly one backend call
 * per user answer. Announces each state change exactly once through the
 * injected `announce` callback (the shell renders it in its single polite
 * region). Concurrent `start()` calls collapse into the running one, and a
 * replaced device fails closed before any mutation.
 */
export class ApplyFlow {
  snapshot: ApplySnapshot;
  private readonly input: ApplyInput;
  private readonly deps: ApplyDeps;
  private readonly announce?: (message: string) => void;
  private readonly notify?: () => void;
  private lastAnnounced: string | null = null;

  constructor(
    input: ApplyInput,
    deps: ApplyDeps,
    opts?: { announce?: (message: string) => void; onChange?: () => void },
  ) {
    this.input = { ...input, allowed: [...input.allowed], entries: [...input.entries] };
    this.deps = deps;
    this.announce = opts?.announce;
    this.notify = opts?.onChange;
    this.snapshot = {
      status: "idle",
      busy: false,
      announcement: "",
      appliedCount: 0,
      rollbackCount: 0,
      blockingPackage: null,
      partialProtection: [],
      error: null,
      outcome: null,
      rolledBackFrom: null,
    };
  }

  /**
   * Starts the apply: re-checks the live device first and never mutates a
   * replaced or unreachable phone. Terminal flows never restart.
   */
  async start(): Promise<void> {
    if (this.snapshot.busy) return;
    if (isApplyTerminal(this.snapshot.status)) return;
    this.set({ status: "verifying", busy: true, error: null });
    this.say("Checking the connected phone before applying the chosen policy.");

    const discovered = await this.deps.discoverDevices();
    if (!discovered.ok) {
      // Fail closed: a dropped cable reports the recovery state, while any
      // other discovery failure keeps its command error for retry.
      if (discovered.error.code === "device-unavailable" || discovered.error.code === "no-device") {
        this.set({ status: "recoverable-disconnect", busy: false, error: discovered.error });
        this.say("The phone stopped responding. Check the USB cable, then try again.");
      } else {
        this.set({ status: "failed", busy: false, error: discovered.error });
        this.say("The phone could not be confirmed. Try again.");
      }
      return;
    }
    const value: unknown = discovered.value;
    let foundSerial: string | null = null;
    if (typeof value === "string") {
      foundSerial = value;
    } else if (typeof value === "object" && value !== null) {
      const serial = (value as Record<string, unknown>)["serial"];
      if (typeof serial === "string") foundSerial = serial;
    }
    if (foundSerial !== this.input.serial) {
      const error: CommandError = {
        code: "stale-device",
        message: "The connected phone is not the one that was inspected.",
        action: "Reconnect the inspected phone, or start a new inspection.",
      };
      this.set({ status: "failed", busy: false, error });
      this.say("This is not the inspected phone. Reconnect the inspected phone, or start a new inspection.");
      return;
    }

    this.set({ status: "running", busy: true });
    this.say("Applying the chosen policy in order.");
    const started = await this.deps.startApply(this.input.serial, this.input.fingerprint, [...this.input.allowed]);
    if (!started.ok) {
      this.set({ status: "failed", busy: false, error: started.error });
      this.say("Applying could not start. Try again.");
      return;
    }
    const dto = parseApplyOutcome(started.value);
    if (dto === null) {
      const error: CommandError = {
        code: "preflight-failed",
        message: "The phone returned an answer the desktop does not understand.",
        action: "Reconnect the phone and try again.",
      };
      this.set({ status: "failed", busy: false, error });
      this.say("The phone returned an answer the desktop does not understand. Reconnect and try again.");
      return;
    }
    this.applyOutcome(dto);
  }

  /** Answers an ordinary-app decision (valid only while deciding). */
  async answerDecision(decision: "continue" | "rollback"): Promise<void> {
    if (this.snapshot.status !== "decision-required") return;
    if (this.snapshot.busy) return;
    await this.answer(decision);
  }

  /** Answers the launcher-chooser wait (valid only while choosing). */
  async answerChooser(decision: "home-confirmed" | "home-cancelled"): Promise<void> {
    if (this.snapshot.status !== "chooser-required") return;
    if (this.snapshot.busy) return;
    await this.answer(decision);
  }

  private async answer(decision: ApplyDecisionName): Promise<void> {
    this.set({ busy: true });
    this.say("Sending your answer to the phone.");
    const result = await this.deps.respondToDecision(this.input.serial, this.input.fingerprint, decision);
    if (!result.ok) {
      this.set({ busy: false, status: "failed", error: result.error });
      this.say("Your answer could not be sent. Try again.");
      return;
    }
    const dto = parseApplyOutcome(result.value);
    if (dto === null) {
      const error: CommandError = {
        code: "preflight-failed",
        message: "The phone returned an answer the desktop does not understand.",
        action: "Reconnect the phone and try again.",
      };
      this.set({ busy: false, status: "failed", error });
      this.say("The phone returned an answer the desktop does not understand. Reconnect and try again.");
      return;
    }
    this.applyOutcome(dto, { viaDecision: decision });
  }

  /**
   * Notes one progress payload (JSON text or object). Progress updates
   * counts and waits, but only a backend outcome completes the flow; a
   * disconnect invalidates further answers.
   */
  noteRemoteProgress(payload: unknown): void {
    if (isApplyTerminal(this.snapshot.status)) return;
    const name = normalizeProgressPayload(decodeJsonText(payload));
    if (name === null) return;
    const detail = extractProgressDetail(payload);
    switch (name) {
      case "started":
        if (this.snapshot.status === "idle") this.set({ status: "running", busy: true });
        else this.set({ busy: true });
        break;
      case "operation-applied":
        this.set({ appliedCount: this.snapshot.appliedCount + 1 });
        break;
      case "decision-required":
        this.set({
          status: "decision-required",
          busy: false,
          blockingPackage: detail ?? this.snapshot.blockingPackage,
        });
        this.sayDecision();
        break;
      case "chooser-required":
        this.set({ status: "chooser-required", busy: false });
        this.say("The phone needs the default launcher chosen by hand. Follow the chooser steps on the phone.");
        break;
      case "rollback-started":
        this.set({ status: "rolling-back", busy: true });
        this.say("Rolling back the completed changes.");
        break;
      case "rollback-applied":
        this.set({ rollbackCount: this.snapshot.rollbackCount + 1 });
        break;
      case "completed":
        // Progress alone never completes: only the backend outcome does.
        break;
      case "disconnected":
        this.set({ status: "recoverable-disconnect", busy: false });
        this.say("The phone stopped responding. Check the USB cable, then try again.");
        break;
      case "inconsistent-state":
        this.set({ status: "inconsistent-state", busy: false });
        this.say("The phone records disagree, so no automatic change is safe.");
        break;
      default:
        break;
    }
  }

  private applyOutcome(dto: ApplyOutcomeDto, opts?: { viaDecision?: ApplyDecisionName }): void {
    const base = {
      outcome: dto.outcome,
      partialProtection: [...dto.partialProtection],
      busy: false as const,
      error: null as CommandError | null,
      rolledBackFrom: null as ApplySnapshot["rolledBackFrom"],
    };
    switch (dto.outcome) {
      case "complete":
        this.set({ ...base, status: "complete", blockingPackage: null });
        this.say(
          dto.partialProtection.length > 0
            ? "Setup is complete and verified on the phone, with partial protection for some paths."
            : "Setup is complete and verified on the phone.",
        );
        break;
      case "decision-required":
        this.set({ ...base, status: "decision-required", blockingPackage: dto.package });
        this.sayDecision();
        break;
      case "chooser-required":
        this.set({ ...base, status: "chooser-required", blockingPackage: null });
        this.say("The phone needs the default launcher chosen by hand. Follow the chooser steps on the phone.");
        break;
      case "rolled-back": {
        // The backend envelope carries no cause: only a direct rolled-back
        // (no preceding user rollback/cancel answer in this flow) has a
        // known store cause (the store hard-failure path). An ordinary user
        // rollback or chooser cancel uses the generic copy.
        const via = opts?.viaDecision ?? null;
        const rolledBackFrom: ApplySnapshot["rolledBackFrom"] =
          via === "rollback" ? "user-rollback" : via === "home-cancelled" ? "chooser-cancel" : "direct";
        this.set({ ...base, status: "rolled-back", blockingPackage: null, rolledBackFrom });
        this.say("Setup was rolled back. The phone is unchanged.");
        break;
      }
      case "recoverable-disconnect":
        this.set({ ...base, status: "recoverable-disconnect", blockingPackage: null });
        this.say("The phone stopped responding. Check the USB cable, then try again.");
        break;
      case "inconsistent-state":
        this.set({ ...base, status: "inconsistent-state", blockingPackage: null });
        this.say("The phone records disagree, so no automatic change is safe.");
        break;
    }
  }

  private sayDecision(): void {
    const found = blockingAppFor(this.input.entries, this.snapshot.blockingPackage);
    this.say(
      found
        ? `One app could not be fully protected: ${found.label} (${found.packageId}). Choose how to continue before the default launcher changes.`
        : "One app could not be fully protected. Choose how to continue before the default launcher changes.",
    );
  }

  private set(partial: Partial<ApplySnapshot>): void {
    this.snapshot = { ...this.snapshot, ...partial };
    this.notify?.();
  }

  private say(message: string): void {
    if (message !== this.lastAnnounced) {
      this.lastAnnounced = message;
      this.snapshot = { ...this.snapshot, announcement: message };
      this.announce?.(message);
      this.notify?.();
    }
  }
}
