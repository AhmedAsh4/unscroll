/**
 * Task 20 scenario support: faithful fake device backend for whole-workflow
 * end-to-end scenarios (no new deps, Node built-in runner only).
 *
 * Contract (mirrors the Rust `FakeAdb` outcome classes, not a policy engine):
 * - Outcome classes are injected by each scenario through explicit failure
 *   flags (`failMutatePackage`, `failVerifyPackage`, `failHome`,
 *   `disconnectMutate`, `failAppOp`, ...), mirroring the flag fields on the
 *   Rust fakes in `desktop/src-tauri/tests/apply_transaction.rs`,
 *   `edit_transaction.rs`, `restore_transaction.rs`, and
 *   `store_maintenance.rs`. No planning, validation, or transaction policy
 *   lives here: the REAL `workspace.ts` machines (`ConnectionFlow`,
 *   `ApplyFlow`, `EditFlow`, `MaintenanceFlow`, `RestoreFlow`) stay the code
 *   under test, and this file only simulates device state plus the backend
 *   outcome envelopes (`{outcome,package,partialProtection}` JSON text).
 * - Device-state simulation mirrors `FakeAdb`'s own simulation
 *   (`suspended` set, `home` component, app-op modes) so scenarios can assert
 *   load-bearing facts: HOME unchanged before a decision, rollback emptying
 *   the suspended set, ordered restore inverses, and USB-debug persistence.
 * - Every mutation mirrors pending journal state to BOTH envelope copies
 *   before execution and applied state to both after verification, exactly
 *   like `MirrorStore::persist`. The shared path is exactly
 *   `/sdcard/Documents/Unscroll/recovery-v1.json`. Tests assert
 *   `privateEnvelopeJson() === sharedEnvelopeJson()` after each step and
 *   inspect `mirrorLog` (pending pairs recorded before the mutating call).
 * - Session answers mirror `transaction::classify` kinds plus the Rust
 *   allowed-actions table (`safeRecoveryActions` in `workspace.ts`).
 * - Icon transport mirrors the Rust `load_app_icon` boundary: cached PNG
 *   bytes as `data:image/png;base64,` URLs, otherwise the typed miss
 *   (`{"missing":true}`); never remote art or guessed brand icons.
 */
import type { AppEntryDto, CommandError, SessionActionName, SessionDto, SessionKindName } from "../../lib/api/types.ts";
import { MAINTENANCE_CONFIRMATION, RESTORE_CONFIRMATION } from "../../lib/api/types.ts";
import type { InvokeResult } from "../../lib/api/invoke.ts";
import type { ApplyInvokeResult, ConnectionDeps } from "../../lib/state/workspace.ts";
import { ConnectionFlow } from "../../lib/state/workspace.ts";

/** Exact shared recovery path both envelope copies mirror. */
export const SHARED_RECOVERY_PATH = "/sdcard/Documents/Unscroll/recovery-v1.json";

/** Device identity mirroring the Rust test fixtures. */
export const SERIAL = "device";
export const FINGERPRINT = "maker/device";
export const MODEL = "Test Phone";
export const MANUFACTURER = "TestMaker";
export const API = 33;

/** HOME components: Unscroll target vs recorded baseline (restore target). */
export const UNSCROLL_HOME = "org.unscroll.launcher/.Main";
export const BASE_HOME = "com.base/.Home";
export const BASELINE_LAUNCHER = "com.base";

/** First journal id, mirroring the Rust deterministic id sequence. */
export const FIRST_JOURNAL_ID = "00000000-0000-4000-8000-000000000000";

/** Tiny valid PNG body used for cached icons (local bytes only). */
const CACHED_PNG_BODY = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
const CACHED_PNG_URL = `data:image/png;base64,${CACHED_PNG_BODY}`;

function ok<T>(value: T): { ok: true; value: T } {
  return { ok: true, value };
}

function fail(code: CommandError["code"], message: string, action: string): { ok: false; error: CommandError } {
  return { ok: false, error: { code, message, action } };
}

export function outcomeJson(outcome: string, pkg: string | null = null, partial: string[] = []): string {
  return JSON.stringify({ outcome, package: pkg, partialProtection: partial });
}

function nextJournalId(index: number): string {
  return `00000000-0000-4000-8000-${index.toString(16).padStart(12, "0")}`;
}

export interface JournalEntry {
  id: string;
  op: string;
  inverse: string;
  state: "pending" | "applied";
}

export interface FakeEnvelope {
  baselineHash: string;
  baselineLauncher: string;
  initialHome: string;
  revision: number;
  journal: JournalEntry[];
  maintenance: "open" | "closed";
  notes: string[];
}

export interface CallRecord {
  command: string;
  detail: string;
}

/** Commands that mutate the device or its recovery copies. */
const MUTATING_COMMANDS: ReadonlySet<string> = new Set([
  "install_bootstrap",
  "start_apply",
  "respond_to_decision",
  "start_edit",
  "open_maintenance",
  "close_maintenance",
  "start_restore",
  "retry_cleanup",
]);

function catalogFixture(): AppEntryDto[] {
  const row = (
    packageId: string,
    label: string,
    extra: Partial<AppEntryDto> = {},
  ): AppEntryDto => ({
    packageId,
    label,
    suspended: false,
    enabled: true,
    protected: false,
    protectedReason: null,
    iconCached: false,
    isStore: false,
    isInstallSource: false,
    ...extra,
  });
  return [
    row("com.keep", "Keep"),
    row("com.blocked", "Blocked App"),
    row("com.store", "Test Store", { isStore: true }),
    row("com.source", "Install Source", { isInstallSource: true }),
    row(BASELINE_LAUNCHER, "Baseline Launcher", {
      protected: true,
      protectedReason: "baseline launcher",
    }),
    row("com.system", "System UI", { protected: true, protectedReason: "system component" }),
    row("com.ambiguous", "Ambiguous", { protected: true, protectedReason: "ambiguous shared role" }),
    row("com.pre", "Pre-suspended", { suspended: true }),
    row("com.icons", "Icon App", { iconCached: true }),
  ];
}

function iconFixture(): Record<string, string | { missing: true } | null> {
  return {
    "com.keep": CACHED_PNG_URL,
    "com.icons": CACHED_PNG_URL,
    "com.blocked": { missing: true },
    "com.store": { missing: true },
    "com.source": { missing: true },
  };
}

const BASELINE_HASH = "baseline-hash-test";

/**
 * Faithful fake phone. Scenarios inject one failure class at a time through
 * the public flags (mirroring one Rust `FakeAdb` configuration each); the
 * default configuration is the happy path.
 */
export class FakePhone {
  entries: AppEntryDto[] = catalogFixture();
  home: string = BASE_HOME;
  appOps: Map<string, string> = new Map();
  usbDebugEnabled = true;
  bootstrapInstalled = false;
  launcherDataCleared = false;

  privateEnvelope: FakeEnvelope | null = null;
  sharedEnvelope: FakeEnvelope | null = null;

  /** Failure injection mirroring the Rust `FakeAdb` fields. */
  failMutatePackage: string | null = null;
  failVerifyPackage: string | null = null;
  failAppOp = false;
  falseAppOp = false;
  failHome = false;
  failHomeRestore = false;
  disconnectMutate = false;
  disconnectVerify = false;
  failUnsuspend = false;
  failCloseMaintenance = false;
  rejectEditPlan = false;
  failCleanupShared = false;
  /** Shared-copy corruption classes for fresh-desktop negative tests. */
  corruptShared = false;
  dropShared = false;
  forkCopies = false;
  /** Session overrides mirroring `classify` cases. */
  rollbackOnly = false;
  restoreReady = false;
  cleanupRemaining = false;
  sessionKindOverride: SessionKindName | null = null;
  /** Packages installed during an open maintenance window. */
  newlyInstalled: string[] = [];

  callLog: CallRecord[] = [];
  homeCalls: string[] = [];
  inverseLog: string[] = [];
  /** (private, shared) envelope pairs captured at each mutate/verify point. */
  mirrorLog: Array<[string | null, string | null]> = [];
  privateReads = 0;
  sharedReads = 0;
  privateApplyAttempts = 0;

  get mutatingCalls(): CallRecord[] {
    return this.callLog.filter((call) => MUTATING_COMMANDS.has(call.command));
  }

  log(command: string, detail = ""): void {
    this.callLog.push({ command, detail });
  }

  privateEnvelopeJson(): string | null {
    return this.privateEnvelope === null ? null : JSON.stringify(this.privateEnvelope);
  }

  sharedEnvelopeJson(): string | null {
    if (this.dropShared) return null;
    if (this.corruptShared) return '{"schema_version":"recovery-v1","checksum":"deadbeef"';
    if (this.forkCopies && this.privateEnvelope !== null && this.sharedEnvelope !== null) {
      const forked = { ...this.sharedEnvelope, revision: this.sharedEnvelope.revision + 1 };
      return JSON.stringify(forked);
    }
    return this.sharedEnvelope === null ? null : JSON.stringify(this.sharedEnvelope);
  }

  /** True when both copies hold identical bytes (the mirror invariant). */
  mirrorsEqual(): boolean {
    return this.privateEnvelopeJson() === this.sharedEnvelopeJson();
  }

  /** Records the mirrored pair at a mutate/verify point (pending first). */
  private mirrorPoint(): void {
    this.mirrorLog.push([this.privateEnvelopeJson(), this.sharedEnvelopeJson()]);
  }

  private writeBoth(envelope: FakeEnvelope): void {
    const clone = (value: FakeEnvelope): FakeEnvelope => JSON.parse(JSON.stringify(value)) as FakeEnvelope;
    this.privateEnvelope = clone(envelope);
    this.sharedEnvelope = clone(envelope);
  }

  private pendingId(): string {
    const journal = (this.privateEnvelope ?? this.sharedEnvelope)?.journal ?? [];
    const pending = journal.find((entry) => entry.state === "pending");
    if (pending !== undefined) return pending.id;
    return nextJournalId(journal.length);
  }

  isSuspended(packageId: string): boolean {
    return this.entries.find((entry) => entry.packageId === packageId)?.suspended === true;
  }

  private setSuspended(packageId: string, suspended: boolean): void {
    const entry = this.entries.find((row) => row.packageId === packageId);
    if (entry !== undefined) entry.suspended = suspended;
  }

  // -- bootstrap + inspection -------------------------------------------

  installBootstrap(): ApplyInvokeResult {
    this.log("install_bootstrap", BASELINE_LAUNCHER);
    this.bootstrapInstalled = true;
    return ok(null);
  }

  inspectResult(): unknown {
    this.log("inspect_device", SERIAL);
    return {
      serial: SERIAL,
      model: MODEL,
      manufacturer: MANUFACTURER,
      api: API,
      fingerprint: FINGERPRINT,
      entries: this.entries.map((entry) => ({ ...entry })),
    };
  }

  iconTransport = (packageId: string): Promise<unknown> => {
    this.log("load_app_icon", packageId);
    const icons = iconFixture();
    const hit = icons[packageId] ?? { missing: true };
    return Promise.resolve(hit);
  };

  // -- session classification (mirrors transaction::classify kinds) ------

  /**
   * Session classification (mirrors `transaction::classify` kinds). With
   * `sharedOnly`, a fresh desktop classifies from the shared copy alone and
   * never consults the private copy — except fork reconciliation, which
   * inherently compares both device copies (a fork is only visible as a
   * disagreement between them; neither copy alone, and no host memory, can
   * prove it). Both copies live on the device, so the comparison consults
   * no host-local state.
   */
  sessionKind(sharedOnly = false): SessionKindName {
    if (this.sessionKindOverride !== null) return this.sessionKindOverride;
    const local = sharedOnly ? null : this.privateEnvelope;
    const shared = this.sharedEnvelope;
    if (this.corruptShared || this.dropShared) {
      if (local !== null || shared !== null) return "blocked-inconsistency";
    }
    if (this.forkCopies && this.privateEnvelope !== null && this.sharedEnvelope !== null) {
      return "blocked-inconsistency";
    }
    if (local?.maintenance === "open" || shared?.maintenance === "open") {
      return "maintenance-recovery";
    }
    const journal = local?.journal ?? shared?.journal ?? [];
    if (journal.some((entry) => entry.state === "pending")) {
      return this.rollbackOnly ? "rollback-only" : "resumable-transaction";
    }
    if (this.cleanupRemaining) return "cleanup-retry";
    if (this.restoreReady) return "restore-ready";
    if (local !== null || shared !== null) return "active-policy";
    return "new-setup";
  }

  sessionResponse(sharedOnly: boolean): SessionDto {
    if (sharedOnly) {
      this.sharedReads += 1;
    } else {
      this.privateReads += 1;
      this.sharedReads += 1;
    }
    const kind = this.sessionKind(sharedOnly);
    const allowedActions: SessionActionName[] = (() => {
      switch (kind) {
        case "new-setup":
          return ["begin-setup"];
        case "active-policy":
          return ["restore", "export-diagnostics"];
        case "maintenance-recovery":
          return ["export-diagnostics"];
        case "resumable-transaction":
          return ["resume", "rollback", "export-diagnostics"];
        case "rollback-only":
          return ["rollback", "export-diagnostics"];
        case "restore-ready":
          return ["restore", "export-diagnostics"];
        case "cleanup-retry":
          return ["retry-cleanup", "export-diagnostics"];
        case "blocked-inconsistency":
          return ["export-diagnostics"];
      }
    })();
    return { kind, guidance: `test ${kind}`, allowedActions };
  }

  // -- invoke-shaped deps for the real workspace machines ----------------

  private checkSerial(serial: string): { ok: false; error: CommandError } | null {
    if (serial !== SERIAL) {
      return fail("stale-device", "The connected phone is not the one that was inspected.", "Reconnect the inspected phone.");
    }
    return null;
  }

  discover(): InvokeResult<{ serial: string }> {
    this.log("discover_devices", SERIAL);
    if (!this.usbDebugEnabled) {
      return fail("device-unavailable", "The phone stopped responding.", "Check the USB cable, then try again.");
    }
    return ok({ serial: SERIAL });
  }

  inspect(serial: string): InvokeResult<unknown> {
    const mismatch = this.checkSerial(serial);
    if (mismatch !== null) return mismatch;
    return ok(this.inspectResult());
  }

  getSession(serial: string, _fingerprint: string, sharedOnly = false): InvokeResult<SessionDto> {
    const mismatch = this.checkSerial(serial);
    if (mismatch !== null) return mismatch;
    this.log(sharedOnly ? "get_session_shared" : "get_session", this.sessionKind(sharedOnly));
    return ok(this.sessionResponse(sharedOnly));
  }

  connectionDeps(sharedOnly = false): ConnectionDeps {
    return {
      discoverDevices: async (): Promise<InvokeResult<{ serial: string }>> => this.discover(),
      inspectDevice: async (serial: string): Promise<InvokeResult<unknown>> => this.inspect(serial),
      getSession: async (serial: string, fingerprint: string): Promise<InvokeResult<SessionDto>> =>
        this.getSession(serial, fingerprint, sharedOnly),
    };
  }

  /** Connects through the REAL ConnectionFlow (install/bootstrap+inspect). */
  async connectNewSetup(sharedOnly = false) {
    const flow = new ConnectionFlow(this.connectionDeps(sharedOnly));
    await flow.connect();
    return flow.snapshot;
  }

  // -- apply (mirrors transaction::apply outcome classes) ----------------

  private baseEnvelope(): FakeEnvelope {
    return {
      baselineHash: BASELINE_HASH,
      baselineLauncher: BASELINE_LAUNCHER,
      initialHome: BASE_HOME,
      revision: 0,
      journal: [],
      maintenance: "closed",
      notes: [],
    };
  }

  private blockedTargets(allowed: string[]): AppEntryDto[] {
    const kept = new Set(allowed);
    return this.entries.filter((entry) => !entry.protected && !kept.has(entry.packageId));
  }

  private isStoreTarget(entry: AppEntryDto): boolean {
    return entry.isStore || entry.isInstallSource;
  }

  startApply(serial: string, _fingerprint: string, allowed: string[]): ApplyInvokeResult {
    const mismatch = this.checkSerial(serial);
    if (mismatch !== null) return mismatch;
    this.log("start_apply", allowed.join(","));
    this.privateApplyAttempts += 1;

    const id = this.pendingId();
    const base = this.privateEnvelope ?? this.sharedEnvelope ?? this.baseEnvelope();
    const pending: FakeEnvelope = JSON.parse(JSON.stringify(base)) as FakeEnvelope;
    if (!pending.journal.some((entry) => entry.id === id)) {
      pending.journal.push({
        id,
        op: `suspend blocked not in allowlist (${allowed.length} kept)`,
        inverse: "unsuspend blocked",
        state: "pending",
      });
    }
    // Invariant: mirrored pending to BOTH copies before execution.
    this.writeBoth(pending);
    this.mirrorPoint();

    if (this.disconnectMutate || this.disconnectVerify) {
      return ok(outcomeJson("recoverable-disconnect"));
    }

    const targets = this.blockedTargets(allowed);
    const requiredStore = targets.find(
      (entry) => this.isStoreTarget(entry) && (this.failMutatePackage === entry.packageId || this.failVerifyPackage === entry.packageId),
    );
    if (requiredStore !== undefined) {
      // Store hard gate: roll back prior changes, never touch HOME.
      for (const entry of targets) this.setSuspended(entry.packageId, entry.packageId === "com.pre");
      const rolled: FakeEnvelope = JSON.parse(JSON.stringify(pending)) as FakeEnvelope;
      rolled.journal = rolled.journal.map((entry) => ({ ...entry, state: "applied" as const }));
      rolled.notes.push("store hard failure rolled back");
      this.writeBoth(rolled);
      this.mirrorPoint();
      return ok(outcomeJson("rolled-back"));
    }

    const ordinary = targets.find((entry) => this.failVerifyPackage === entry.packageId);
    if (ordinary !== undefined && this.failMutatePackage === ordinary.packageId) {
      // Mutate+verify failure on an ordinary app still rolls back (Rust:
      // command failure with proven forward state is recorded + rolled back).
      for (const entry of targets) this.setSuspended(entry.packageId, entry.packageId === "com.pre");
      const rolled: FakeEnvelope = JSON.parse(JSON.stringify(pending)) as FakeEnvelope;
      rolled.notes.push("ordinary mutate failure rolled back");
      this.writeBoth(rolled);
      return ok(outcomeJson("rolled-back"));
    }
    if (ordinary !== undefined) {
      // Ordinary verification failure pauses BEFORE home: no HOME call.
      return ok(outcomeJson("decision-required", ordinary.packageId));
    }

    if (this.failHome) {
      for (const entry of targets) this.setSuspended(entry.packageId, true);
      return ok(outcomeJson("chooser-required"));
    }

    // Success path (HOME shell path): suspend blocked, verify, set HOME.
    for (const entry of targets) {
      if (this.failUnsuspend && entry.suspended) continue;
      this.setSuspended(entry.packageId, true);
    }
    this.mirrorPoint();
    this.homeCalls.push(`Home ${UNSCROLL_HOME}`);
    this.home = UNSCROLL_HOME;
    const applied: FakeEnvelope = JSON.parse(JSON.stringify(pending)) as FakeEnvelope;
    applied.journal = applied.journal.map((entry) => ({ ...entry, state: "applied" as const }));
    applied.revision += 1;
    const partial: string[] = [];
    if (this.failAppOp || this.falseAppOp) {
      partial.push("Install-source app-op could not be restricted on com.source");
      applied.notes.push("partial protection com.source app-op");
    }
    this.writeBoth(applied);
    this.mirrorPoint();
    return ok(outcomeJson("complete", null, partial));
  }

  respondToDecision(_serial: string, _fingerprint: string, decision: string): ApplyInvokeResult {
    this.log("respond_to_decision", decision);
    const pending: FakeEnvelope | null = this.privateEnvelope ?? this.sharedEnvelope;
    if (decision === "continue") {
      if (this.failVerifyPackage !== null) {
        const pkg = this.failVerifyPackage;
        return ok(outcomeJson("decision-required", pkg));
      }
      const applied: FakeEnvelope = JSON.parse(JSON.stringify(pending ?? this.baseEnvelope())) as FakeEnvelope;
      applied.journal = applied.journal.map((entry) => ({ ...entry, state: "applied" as const }));
      // Mirrors the Rust resume marker: the continued envelope records the
      // failed ordinary step plus the launcher-policy HOME step.
      applied.notes.push("failed ordinary step continued; launcher_policy home verified");
      applied.revision += 1;
      this.homeCalls.push(`Home ${UNSCROLL_HOME}`);
      this.home = UNSCROLL_HOME;
      this.writeBoth(applied);
      this.mirrorPoint();
      return ok(outcomeJson("complete"));
    }
    if (decision === "rollback") {
      for (const entry of this.entries) {
        if (!entry.protected) this.setSuspended(entry.packageId, entry.packageId === "com.pre");
      }
      const rolled: FakeEnvelope = JSON.parse(JSON.stringify(pending ?? this.baseEnvelope())) as FakeEnvelope;
      rolled.notes.push("user rollback verified");
      this.writeBoth(rolled);
      this.mirrorPoint();
      return ok(outcomeJson("rolled-back"));
    }
    if (decision === "home-confirmed") {
      if (this.failHome) return ok(outcomeJson("chooser-required"));
      // Confirming an already-executed HOME shell step never duplicates it:
      // no new journal id, no additional HOME call. In the chooser fallback
      // the user chose Unscroll by hand, so the recorded HOME is Unscroll
      // without a shell mutation from this confirmation.
      const applied: FakeEnvelope = JSON.parse(JSON.stringify(pending ?? this.baseEnvelope())) as FakeEnvelope;
      applied.journal = applied.journal.map((entry) => ({ ...entry, state: "applied" as const }));
      applied.revision += 1;
      this.home = UNSCROLL_HOME;
      this.writeBoth(applied);
      this.mirrorPoint();
      return ok(outcomeJson("complete"));
    }
    if (decision === "home-cancelled") {
      // Cancellation re-enters the chooser with the same pending id and no
      // HOME change (mirrors the Rust pending-home-cancellation case).
      return ok(outcomeJson("chooser-required"));
    }
    return fail("invalid-decision", "Unknown test decision.", "Try again.");
  }

  applyDeps() {
    return {
      discoverDevices: async () => this.discover(),
      startApply: async (serial: string, fingerprint: string, allowed: string[]) =>
        this.startApply(serial, fingerprint, allowed),
      respondToDecision: async (serial: string, fingerprint: string, decision: "continue" | "rollback" | "home-confirmed" | "home-cancelled") =>
        this.respondToDecision(serial, fingerprint, decision),
    };
  }

  // -- edit (mirrors transaction::edit: delta only, baseline preserved) ---

  startEdit(serial: string, _fingerprint: string, allowed: string[]): ApplyInvokeResult {
    const mismatch = this.checkSerial(serial);
    if (mismatch !== null) return mismatch;
    this.log("start_edit", allowed.join(","));
    if (!this.usbDebugEnabled) {
      return fail("device-unavailable", "The phone stopped responding.", "Check the USB cable, then try again.");
    }
    if (this.rejectEditPlan) {
      return fail("plan-rejected", "The edit plan was rejected. Reconnect and reconcile the session.", "Reconcile the session, then retry.");
    }
    if (this.disconnectMutate || this.disconnectVerify) {
      return fail("device-unavailable", "The phone stopped responding.", "Check the USB cable, then try again.");
    }
    const kept = new Set(allowed);
    for (const entry of this.entries) {
      if (entry.protected) continue;
      this.setSuspended(entry.packageId, !kept.has(entry.packageId) || entry.packageId === "com.pre");
    }
    const current = this.privateEnvelope ?? this.sharedEnvelope ?? this.baseEnvelope();
    const next: FakeEnvelope = JSON.parse(JSON.stringify(current)) as FakeEnvelope;
    next.revision += 1;
    next.notes.push(`edit delta allowlist=${[...kept].sort().join(",")}`);
    // The baseline is never replaced by an edit.
    next.baselineHash = BASELINE_HASH;
    next.baselineLauncher = BASELINE_LAUNCHER;
    this.writeBoth(next);
    this.mirrorPoint();
    return ok(null);
  }

  editDeps() {
    return {
      discoverDevices: async () => this.discover(),
      startEdit: async (serial: string, fingerprint: string, allowed: string[]) =>
        this.startEdit(serial, fingerprint, allowed),
    };
  }

  // -- maintenance (mirrors transaction::open/close) ----------------------

  openMaintenance(serial: string, _fingerprint: string, confirmation: string): ApplyInvokeResult {
    const mismatch = this.checkSerial(serial);
    if (mismatch !== null) return mismatch;
    this.log("open_maintenance", confirmation);
    if (confirmation !== MAINTENANCE_CONFIRMATION) {
      return fail("invalid-confirmation", "The maintenance phrase was not typed exactly.", `Type ${MAINTENANCE_CONFIRMATION} exactly.`);
    }
    const current = this.privateEnvelope ?? this.sharedEnvelope ?? this.baseEnvelope();
    const next: FakeEnvelope = JSON.parse(JSON.stringify(current)) as FakeEnvelope;
    next.maintenance = "open";
    next.notes.push("maintenance window open");
    this.writeBoth(next);
    this.mirrorPoint();
    return ok(null);
  }

  closeMaintenance(serial: string, _fingerprint: string, approved: string[], scanned: string[]): ApplyInvokeResult {
    const mismatch = this.checkSerial(serial);
    if (mismatch !== null) return mismatch;
    this.log("close_maintenance", `approved=${approved.join(",")} scanned=${scanned.join(",")}`);
    if (this.disconnectMutate || this.disconnectVerify) {
      return fail("device-unavailable", "The phone stopped responding.", "Check the USB cable, then close maintenance again.");
    }
    if (this.failCloseMaintenance) {
      return fail("preflight-failed", "The close could not be verified.", "Close maintenance again.");
    }
    // Rescan re-blocks: newly installed unapproved apps are suspended.
    const approvedSet = new Set(approved);
    for (const packageId of this.newlyInstalled) {
      if (!approvedSet.has(packageId)) this.setSuspended(packageId, true);
    }
    const current = this.privateEnvelope ?? this.sharedEnvelope ?? this.baseEnvelope();
    const next: FakeEnvelope = JSON.parse(JSON.stringify(current)) as FakeEnvelope;
    next.maintenance = "closed";
    next.revision += 1;
    next.notes.push("maintenance closed; store restrictions re-applied");
    this.writeBoth(next);
    this.mirrorPoint();
    return ok(null);
  }

  maintenanceDeps() {
    return {
      discoverDevices: async () => this.discover(),
      openMaintenance: async (serial: string, fingerprint: string, confirmation: string) =>
        this.openMaintenance(serial, fingerprint, confirmation),
      closeMaintenance: async (serial: string, fingerprint: string, approved: string[], scanned: string[]) =>
        this.closeMaintenance(serial, fingerprint, approved, scanned),
    };
  }

  // -- restore (mirrors transaction::restore ordered inverses) ------------

  startRestore(serial: string, _fingerprint: string, confirmation: string): ApplyInvokeResult {
    const mismatch = this.checkSerial(serial);
    if (mismatch !== null) return mismatch;
    this.log("start_restore", confirmation);
    if (confirmation !== RESTORE_CONFIRMATION) {
      return fail("invalid-confirmation", "The restore phrase was not typed exactly.", `Type ${RESTORE_CONFIRMATION} exactly.`);
    }
    if (!this.usbDebugEnabled) {
      return fail("device-unavailable", "The phone stopped responding.", "Check the USB cable, then try again.");
    }
    const envelope = this.privateEnvelope ?? this.sharedEnvelope;
    if (envelope === null) {
      return fail("restore-blocked", "No Unscroll policy was found to restore.", "Reconnect and reconcile the session.");
    }
    // Ordered inverses: reverse of application order (home first, then
    // app-ops, then suspensions), each verified before the next.
    this.inverseLog.push("inverse app_op allow com.source");
    this.appOps.set("com.source:REQUEST_INSTALL_PACKAGES", "allow");
    this.inverseLog.push("inverse unsuspend blocked");
    for (const entry of this.entries) {
      if (!entry.protected) this.setSuspended(entry.packageId, entry.packageId === "com.pre");
    }
    this.mirrorPoint();
    if (this.failHomeRestore) {
      return ok(outcomeJson("chooser-required"));
    }
    this.inverseLog.push(`inverse home ${BASE_HOME}`);
    this.homeCalls.push(`Home ${BASE_HOME}`);
    this.home = BASE_HOME;
    this.mirrorPoint();
    if (this.failCleanupShared) {
      // Everything restored except final cleanup (mirrors cleanup-retry).
      return ok(JSON.stringify({ outcome: "cleanup-retry" }));
    }
    this.privateEnvelope = null;
    this.sharedEnvelope = null;
    this.cleanupRemaining = false;
    this.restoreReady = false;
    return ok(JSON.stringify({ outcome: "complete" }));
  }

  restoreChooserDecision(_serial: string, _fingerprint: string, decision: string): ApplyInvokeResult {
    this.log("respond_to_decision", decision);
    if (decision === "home-confirmed") {
      if (this.failHomeRestore) return ok(JSON.stringify({ outcome: "chooser-required" }));
      this.inverseLog.push(`inverse home ${BASE_HOME}`);
      this.homeCalls.push(`Home ${BASE_HOME}`);
      this.home = BASE_HOME;
      if (this.failCleanupShared) return ok(JSON.stringify({ outcome: "cleanup-retry" }));
      this.privateEnvelope = null;
      this.sharedEnvelope = null;
      return ok(JSON.stringify({ outcome: "complete" }));
    }
    return ok(JSON.stringify({ outcome: "chooser-required" }));
  }

  retryCleanup(serial: string, _fingerprint: string): ApplyInvokeResult {
    const mismatch = this.checkSerial(serial);
    if (mismatch !== null) return mismatch;
    this.log("retry_cleanup", SERIAL);
    if (this.failCleanupShared) {
      return fail("device-unavailable", "Final cleanup could not remove the shared copy.", "Retry cleanup again.");
    }
    this.privateEnvelope = null;
    this.sharedEnvelope = null;
    this.cleanupRemaining = false;
    return ok(null);
  }

  restoreDeps() {
    return {
      discoverDevices: async () => this.discover(),
      startRestore: async (serial: string, fingerprint: string, confirmation: string) =>
        this.startRestore(serial, fingerprint, confirmation),
      respondToDecision: async (serial: string, fingerprint: string, decision: "home-confirmed" | "home-cancelled") =>
        this.restoreChooserDecision(serial, fingerprint, decision),
      retryCleanup: async (serial: string, fingerprint: string) => this.retryCleanup(serial, fingerprint),
    };
  }

  // -- launcher-data-clear + USB-debug host operations --------------------

  /**
   * Mirrors clearing Unscroll Launcher data on the phone: the private
   * recovery copy and host-side allowlist vanish, while the shared envelope
   * and the on-device policy (suspensions, HOME) persist untouched.
   */
  clearLauncherData(): void {
    this.log("clear_launcher_data", SHARED_RECOVERY_PATH);
    this.privateEnvelope = null;
    this.launcherDataCleared = true;
  }

  setUsbDebug(enabled: boolean): void {
    this.log(enabled ? "enable_usb_debug" : "disable_usb_debug", SERIAL);
    this.usbDebugEnabled = enabled;
  }
}

/** Creates a bootstrapped scenario phone (install step done, policy fresh). */
export function createScenarioPhone(): FakePhone {
  const phone = new FakePhone();
  phone.installBootstrap();
  return phone;
}
