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
/* currently carries only the `iconCached` flag per entry — no icon   */
/* bytes cross into the webview, so there is nothing to decode here.  */
/* `AppRow` therefore takes an `iconSrc: string | null` prop: a       */
/* resolved local/object URL when a future Task 15 resource mapping   */
/* provides one, otherwise null. A null (or failed) source renders    */
/* the neutral local fallback from `fallbackInitial` — the app's      */
/* initial letter plus its label. Never substitute remote art, a      */
/* brand catalog, or guessed icons.                                   */
/* ------------------------------------------------------------------ */

/** Chooser filter. Kept = kept incl. protected; Blocked = not kept. */
export type SelectionFilter = "all" | "kept" | "blocked";

/** Allowlist selection keyed by packageId (labels may repeat). */
export type SelectionMap = Record<string, boolean>;

/** Status pill states: chooser rows plus every review group. */
export type PillState = "kept" | "blocked" | "protected" | "store" | "unsupported";

/**
 * Store/sideload-source heuristic. No store or install-source DTOs cross
 * the inspect boundary, so review grouping falls back to whole-token
 * matching against `packageId + label`: the text is lowercased, split on
 * non-alphanumeric boundaries, and matched exactly against STORE_TOKENS.
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
  if (isStoreLike(entry)) return "store";
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
    } else if (isStoreLike(entry)) {
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

/** Inspected data handed from Connect to the chooser (identity stays out). */
export interface InspectedInfo {
  entries: AppEntryDto[];
  session: SessionDto | null;
  connection: ConnectionState;
}
