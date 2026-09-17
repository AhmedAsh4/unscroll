/**
 * Task 19 RED: policy-management state in workspace.ts.
 *
 * Runs on the Node built-in test runner (no added dependencies):
 *   node --test src/tests/task19-policy-management.test.ts
 *
 * Covers the Task 19 acceptance checklist via pure helpers and flow
 * machines: reconciled-session routing/gating for every session kind,
 * edit-delta computation, maintenance confirmation gating, exit
 * interception, maintenance-recovery close-first, restore confirmation
 * exactness, restore progress plus cleanup retry, blocked-inconsistency
 * and missing-envelope safe-only actions, diagnostic preview redaction,
 * export-path validation messaging, and no edit/restore before
 * reconciliation.
 */
import { describe, it } from "node:test";
import assert from "node:assert/strict";

import {
  DiagnosticsFlow,
  EditFlow,
  MaintenanceFlow,
  RestoreFlow,
  canEditPolicy,
  canExitMaintenance,
  canRestorePolicy,
  computeEditDelta,
  diagnosticRedactionProblems,
  exportPathCode,
  exportPathProblem,
  isMaintenanceConfirmation,
  isPolicyReconciled,
  isRestoreConfirmation,
  maintenanceConfirmReason,
  parseDiagnosticPreview,
  parseRestoreOutcome,
  policyActionsFor,
  policyFlowKeyFor,
  policyGateReason,
  requiresCloseFirst,
  restoreConfirmReason,
  safeRecoveryActions,
  type ApplyInvokeResult,
  type ConnectionState,
  type SessionDto,
} from "../lib/state/workspace.ts";
import type { CommandError, DiagnosticPreviewDto, SessionKindName } from "../lib/api/types.ts";
import { MAINTENANCE_CONFIRMATION, RESTORE_CONFIRMATION } from "../lib/api/types.ts";

function session(kind: SessionKindName): SessionDto {
  return { kind, guidance: `test ${kind}`, allowedActions: [] };
}

function fail(code: CommandError["code"]): ApplyInvokeResult {
  return { ok: false, error: { code, message: `test ${code}`, action: "Try again." } };
}

function ok(value: unknown): ApplyInvokeResult {
  return { ok: true, value };
}

function restoreJson(outcome: string): string {
  return JSON.stringify({ outcome });
}

function previewDto(overrides: Partial<DiagnosticPreviewDto> = {}): DiagnosticPreviewDto {
  return {
    deviceModel: "Test Phone",
    fingerprintRedacted: "ABCDEF12…",
    allowlistCount: 3,
    initialPackageCount: 40,
    operations: ["package_suspension applied", "home applied"],
    errors: [],
    warnings: ["one pending entry"],
    redactedEnvelope: '{"serial":"REDACTED","revision":3}',
    ...overrides,
  };
}

describe("reconciled sessions route to policy actions instead of a fresh setup", () => {
  it("reconciles only a non-new-setup session on a settled connection", () => {
    const kinds: SessionKindName[] = [
      "active-policy",
      "maintenance-recovery",
      "resumable-transaction",
      "rollback-only",
      "restore-ready",
      "cleanup-retry",
      "blocked-inconsistency",
    ];
    for (const kind of kinds) {
      assert.equal(isPolicyReconciled("ready", session(kind)), true, kind);
      assert.equal(isPolicyReconciled("recovery-required", session(kind)), true, kind);
    }
    assert.equal(isPolicyReconciled("maintenance-recovery", session("maintenance-recovery")), true);
    assert.equal(isPolicyReconciled("ready", session("new-setup")), false, "new-setup never routes to policy");
    assert.equal(isPolicyReconciled("ready", null), false, "missing session never routes to policy");
    assert.equal(isPolicyReconciled("idle", session("active-policy")), false);
    assert.equal(isPolicyReconciled("checking", session("active-policy")), false);
    assert.equal(isPolicyReconciled("inspecting", session("active-policy")), false);
    assert.equal(isPolicyReconciled("connected", session("active-policy")), false);
  });

  it("offers each reconciled session class only its proven-safe actions", () => {
    assert.deepEqual(
      [...policyActionsFor("active-policy")].sort(),
      ["diagnostics", "edit", "maintenance", "restore"],
    );
    const recovery = policyActionsFor("maintenance-recovery");
    assert.ok(recovery.includes("close-maintenance"), "recovery offers a verified close");
    assert.ok(recovery.includes("diagnostics"), "recovery offers diagnostics");
    assert.equal(recovery.includes("edit"), false, "recovery never offers edit first");
    assert.equal(recovery.includes("restore"), false, "recovery never offers restore first");
    assert.equal(recovery.includes("maintenance"), false, "recovery never offers a new window first");
    assert.deepEqual(
      [...policyActionsFor("restore-ready")].sort(),
      ["diagnostics", "restore"],
    );
    assert.deepEqual(
      [...policyActionsFor("cleanup-retry")].sort(),
      ["diagnostics", "retry-cleanup"],
    );
    assert.deepEqual(policyActionsFor("blocked-inconsistency"), ["export-diagnostics"]);
    assert.deepEqual(policyActionsFor("new-setup"), ["begin-setup"]);
    const resumable = policyActionsFor("resumable-transaction");
    assert.ok(resumable.includes("resume") && resumable.includes("rollback"), "resumable keeps resume/rollback");
    const rollbackOnly = policyActionsFor("rollback-only");
    assert.ok(rollbackOnly.includes("rollback"), "rollback-only keeps rollback");
    assert.equal(rollbackOnly.includes("resume"), false, "rollback-only never offers resume");
  });
});

describe("no edit or restore affordance until reconciliation succeeds", () => {
  it("gates edit on a reconciled active policy only", () => {
    assert.equal(canEditPolicy("ready", session("active-policy")), true);
    assert.equal(canEditPolicy("recovery-required", session("active-policy")), true);
    assert.equal(canEditPolicy("ready", session("new-setup")), false);
    assert.equal(canEditPolicy("ready", null), false);
    assert.equal(canEditPolicy("idle", session("active-policy")), false);
    assert.equal(canEditPolicy("maintenance-recovery", session("maintenance-recovery")), false);
    assert.equal(canEditPolicy("ready", session("restore-ready")), false);
    assert.equal(canEditPolicy("ready", session("blocked-inconsistency")), false);
  });

  it("gates restore on reconciled active-policy and restore-ready sessions", () => {
    assert.equal(canRestorePolicy("ready", session("active-policy")), true);
    assert.equal(canRestorePolicy("recovery-required", session("restore-ready")), true);
    assert.equal(canRestorePolicy("ready", session("new-setup")), false);
    assert.equal(canRestorePolicy("ready", null), false);
    assert.equal(canRestorePolicy("idle", session("restore-ready")), false);
    assert.equal(canRestorePolicy("maintenance-recovery", session("maintenance-recovery")), false);
    assert.equal(canRestorePolicy("ready", session("cleanup-retry")), false);
    assert.equal(canRestorePolicy("ready", session("blocked-inconsistency")), false);
  });

  it("explains every gate with a reason instead of a silent disabled control", () => {
    assert.equal(policyGateReason("ready", session("active-policy")), null);
    for (const [connection, sess] of [
      ["idle", session("active-policy")],
      ["ready", null],
      ["ready", session("new-setup")],
      ["maintenance-recovery", session("maintenance-recovery")],
    ] as Array<[ConnectionState, SessionDto | null]>) {
      const reason = policyGateReason(connection, sess);
      assert.ok(typeof reason === "string" && reason.length > 0, `${connection} explains its gate`);
    }
  });
});

describe("edit shows only the planned delta and preserves the baseline", () => {
  it("computes added-to-keep and added-to-block, sorted", () => {
    assert.deepEqual(computeEditDelta(["a", "b"], ["b", "c"]), {
      addedToKeep: ["c"],
      addedToBlock: ["a"],
    });
    assert.deepEqual(computeEditDelta(["b", "a"], ["c", "a", "b"]), {
      addedToKeep: ["c"],
      addedToBlock: [],
    });
    assert.deepEqual(computeEditDelta(["a"], ["a"]), { addedToKeep: [], addedToBlock: [] });
  });

  it("surfaces a plan-rejected failure with server guidance and never retries silently", async () => {
    const announced: string[] = [];
    let calls = 0;
    const flow = new EditFlow(
      { serial: "S1", fingerprint: "FP1", allowed: ["com.example.keep"], entries: [] },
      {
        discoverDevices: async () => ok({ serial: "S1" }),
        startEdit: async () => {
          calls += 1;
          return fail("plan-rejected");
        },
      },
      { announce: (message) => void announced.push(message) },
    );
    await flow.start();
    assert.equal(calls, 1, "exactly one backend call, no silent retry");
    assert.equal(flow.snapshot.status, "failed");
    assert.equal(flow.snapshot.error?.code, "plan-rejected");
    assert.ok(announced.length >= 1, "failure is announced, never silent");
  });

  it("collapses concurrent edit starts and guards terminal states", async () => {
    let calls = 0;
    const flow = new EditFlow(
      { serial: "S1", fingerprint: "FP1", allowed: ["a"], entries: [] },
      {
        discoverDevices: async () => {
          await new Promise((resolve) => setTimeout(resolve, 10));
          return ok({ serial: "S1" });
        },
        startEdit: async () => {
          calls += 1;
          return ok("{}");
        },
      },
    );
    await Promise.all([flow.start(), flow.start()]);
    assert.equal(calls, 1, "concurrent starts collapse into one backend call");
    await flow.start();
    assert.equal(calls, 1, "terminal flows never restart");
    assert.equal(flow.snapshot.status, "complete");
  });
});

describe("maintenance requires the typed warning and a verified close", () => {
  it("accepts only the exact maintenance confirmation", () => {
    assert.equal(isMaintenanceConfirmation(MAINTENANCE_CONFIRMATION), true);
    assert.equal(MAINTENANCE_CONFIRMATION, "OPEN STORE MAINTENANCE");
    assert.equal(isMaintenanceConfirmation("open store maintenance"), false);
    assert.equal(isMaintenanceConfirmation("OPEN STORE MAINTENANCE "), false);
    assert.equal(isMaintenanceConfirmation(" OPEN STORE MAINTENANCE"), false);
    assert.equal(isMaintenanceConfirmation(""), false);
    assert.equal(maintenanceConfirmReason(MAINTENANCE_CONFIRMATION), null);
    const reason = maintenanceConfirmReason("almost");
    assert.ok(typeof reason === "string" && reason.includes("OPEN STORE MAINTENANCE"), "mismatch names the exact phrase");
  });

  it("never calls open on a confirmation mismatch", async () => {
    let calls = 0;
    const flow = new MaintenanceFlow(
      { serial: "S1", fingerprint: "FP1" },
      {
        discoverDevices: async () => ok({ serial: "S1" }),
        openMaintenance: async () => {
          calls += 1;
          return ok("{}");
        },
        closeMaintenance: async () => ok("{}"),
      },
    );
    await flow.open("almost");
    assert.equal(calls, 0, "mismatch never reaches the backend");
    assert.equal(flow.snapshot.opened, false);
    assert.equal(flow.snapshot.error?.code, "invalid-confirmation");
  });

  it("opens on the exact phrase and intercepts ordinary exit until verified close", async () => {
    const announced: string[] = [];
    const flow = new MaintenanceFlow(
      { serial: "S1", fingerprint: "FP1" },
      {
        discoverDevices: async () => ok({ serial: "S1" }),
        openMaintenance: async () => ok("{}"),
        closeMaintenance: async () => ok("{}"),
      },
      { announce: (message) => void announced.push(message) },
    );
    assert.equal(canExitMaintenance(flow.snapshot), true, "closed maintenance exits freely");
    await flow.open("OPEN STORE MAINTENANCE");
    assert.equal(flow.snapshot.status, "open");
    assert.equal(flow.snapshot.opened, true);
    assert.equal(canExitMaintenance(flow.snapshot), false, "open maintenance intercepts ordinary exit");
    assert.ok(announced.length >= 1, "open is announced");
    await flow.close([], []);
    assert.equal(flow.snapshot.status, "closed");
    assert.equal(flow.snapshot.opened, false);
    assert.equal(canExitMaintenance(flow.snapshot), true, "verified close releases the exit guard");
  });

  it("window-close integration: a failed verified close keeps the user informed, never exits silently", async () => {
    const announced: string[] = [];
    const flow = new MaintenanceFlow(
      { serial: "S1", fingerprint: "FP1" },
      {
        discoverDevices: async () => ok({ serial: "S1" }),
        openMaintenance: async () => ok("{}"),
        closeMaintenance: async () => fail("maintenance-blocked"),
      },
      { announce: (message) => void announced.push(message) },
    );
    await flow.open("OPEN STORE MAINTENANCE");
    assert.equal(canExitMaintenance(flow.snapshot), false, "close requested because exit is guarded");
    // The host intercepts the window close and requests a verified close first.
    await flow.close([], []);
    assert.equal(flow.snapshot.status, "failed", "failure lands in a visible failed state");
    assert.equal(flow.snapshot.opened, true, "the window is still recorded open");
    assert.equal(canExitMaintenance(flow.snapshot), false, "exit stays guarded after a failed close");
    assert.equal(flow.snapshot.error?.code, "maintenance-blocked");
    assert.ok(announced.length >= 2, "the failed close is announced, never silent");
  });

  it("maintenance-recovery sessions close first before edit, restore, or a new window", () => {
    assert.equal(requiresCloseFirst("maintenance-recovery", session("maintenance-recovery")), true);
    assert.equal(requiresCloseFirst("recovery-required", session("active-policy")), false);
    assert.equal(requiresCloseFirst("ready", session("active-policy")), false);
    assert.equal(canEditPolicy("maintenance-recovery", session("maintenance-recovery")), false);
    assert.equal(canRestorePolicy("maintenance-recovery", session("maintenance-recovery")), false);
  });
});

describe("restore requires the exact typed confirmation", () => {
  it("accepts only the exact restore confirmation", () => {
    assert.equal(isRestoreConfirmation(RESTORE_CONFIRMATION), true);
    assert.equal(RESTORE_CONFIRMATION, "RESTORE MY PHONE");
    assert.equal(isRestoreConfirmation("restore my phone"), false);
    assert.equal(isRestoreConfirmation("RESTORE MY PHONE "), false);
    assert.equal(isRestoreConfirmation(" RESTORE MY PHONE"), false);
    assert.equal(isRestoreConfirmation(""), false);
    assert.equal(restoreConfirmReason(RESTORE_CONFIRMATION), null);
    const reason = restoreConfirmReason("RESTORE");
    assert.ok(typeof reason === "string" && reason.includes("RESTORE MY PHONE"), "mismatch names the exact phrase");
  });

  it("parses every terminal restore outcome name", () => {
    for (const name of ["complete", "chooser-required", "cleanup-retry", "blocked", "recoverable-disconnect"]) {
      assert.equal(parseRestoreOutcome(restoreJson(name))?.outcome, name);
    }
    assert.equal(parseRestoreOutcome("complete"), null, "bare names never parse");
    assert.equal(parseRestoreOutcome("garbage"), null);
    assert.equal(parseRestoreOutcome(restoreJson("rolled-back")), null, "apply-only outcomes never parse as restore");
  });

  it("mismatch blocks with a reason and never starts the backend", async () => {
    let calls = 0;
    const flow = new RestoreFlow(
      { serial: "S1", fingerprint: "FP1" },
      {
        discoverDevices: async () => ok({ serial: "S1" }),
        startRestore: async () => {
          calls += 1;
          return ok(restoreJson("complete"));
        },
        respondToDecision: async () => ok(restoreJson("complete")),
        retryCleanup: async () => ok('{"cleaned":true}'),
      },
    );
    await flow.start("RESTORE");
    assert.equal(calls, 0, "mismatch never reaches the backend");
    assert.equal(flow.snapshot.error?.code, "invalid-confirmation");
    assert.equal(flow.snapshot.status, "failed");
  });

  it("tracks inverse progress and completes only on the backend outcome", async () => {
    const deps = () => ({
      discoverDevices: async () => ok({ serial: "S1" }),
      startRestore: async () => ok(restoreJson("complete")),
      respondToDecision: async () => ok(restoreJson("complete")),
      retryCleanup: async () => ok('{"cleaned":true}'),
    });
    const progress = new RestoreFlow({ serial: "S1", fingerprint: "FP1" }, deps());
    progress.noteRemoteProgress(JSON.stringify({ event: "restore-started", serial: "S1", detail: null }));
    assert.equal(progress.snapshot.status, "running");
    progress.noteRemoteProgress(JSON.stringify({ event: "operation-applied", serial: "S1", detail: null }));
    progress.noteRemoteProgress(JSON.stringify({ event: "operation-applied", serial: "S1", detail: null }));
    assert.equal(progress.snapshot.restoredCount, 2);
    progress.noteRemoteProgress(JSON.stringify({ event: "restore-completed", serial: "S1", detail: null }));
    assert.equal(progress.snapshot.status, "running", "progress alone never completes the restore");
    const flow = new RestoreFlow({ serial: "S1", fingerprint: "FP1" }, deps());
    await flow.start("RESTORE MY PHONE");
    assert.equal(flow.snapshot.status, "complete");
    assert.equal(flow.snapshot.outcome, "complete");
  });

  it("retains cleanup-retry with a retry action instead of finishing early", async () => {
    let retries = 0;
    const flow = new RestoreFlow(
      { serial: "S1", fingerprint: "FP1" },
      {
        discoverDevices: async () => ok({ serial: "S1" }),
        startRestore: async () => ok(restoreJson("cleanup-retry")),
        respondToDecision: async () => ok(restoreJson("complete")),
        retryCleanup: async () => {
          retries += 1;
          return ok('{"cleaned":true}');
        },
      },
    );
    await flow.start("RESTORE MY PHONE");
    assert.equal(flow.snapshot.status, "cleanup-retry");
    assert.equal(flow.snapshot.outcome, "cleanup-retry");
    await flow.retry();
    assert.equal(retries, 1);
    assert.equal(flow.snapshot.status, "complete");
    assert.equal(flow.snapshot.outcome, "complete");
  });

  it("answers the launcher chooser through the backend and maps disconnects", async () => {
    const flow = new RestoreFlow(
      { serial: "S1", fingerprint: "FP1" },
      {
        discoverDevices: async () => ok({ serial: "S1" }),
        startRestore: async () => ok(restoreJson("chooser-required")),
        respondToDecision: async () => ok(restoreJson("complete")),
        retryCleanup: async () => ok('{"cleaned":true}'),
      },
    );
    await flow.start("RESTORE MY PHONE");
    assert.equal(flow.snapshot.status, "chooser-required");
    await flow.answerChooser("home-confirmed");
    assert.equal(flow.snapshot.status, "complete");
    flow.noteRemoteProgress("disconnected");
    assert.equal(flow.snapshot.status, "complete", "terminal flows ignore later progress");
  });
});

describe("blocked histories offer only safe recovery guidance", () => {
  it("limits blocked-inconsistency to diagnostic export", () => {
    assert.deepEqual(safeRecoveryActions(session("blocked-inconsistency")), ["export-diagnostics"]);
    assert.deepEqual(safeRecoveryActions(null), ["export-diagnostics"]);
  });

  it("keeps rollback-only and resumable guidance within safe bounds", () => {
    assert.ok(safeRecoveryActions(session("rollback-only")).includes("rollback"));
    assert.equal(safeRecoveryActions(session("rollback-only")).includes("restore"), false);
    assert.ok(safeRecoveryActions(session("resumable-transaction")).includes("resume"));
    assert.equal(safeRecoveryActions(session("resumable-transaction")).includes("restore"), false);
  });
});

describe("diagnostics preview redaction and export-path validation", () => {
  it("parses the preview from JSON text or an object", () => {
    const dto = previewDto();
    assert.deepEqual(parseDiagnosticPreview(JSON.stringify(dto)), dto);
    assert.deepEqual(parseDiagnosticPreview(dto), dto);
    assert.equal(parseDiagnosticPreview("garbage"), null);
    assert.equal(parseDiagnosticPreview(null), null);
  });

  it("flags unredacted fingerprints and envelopes", () => {
    assert.deepEqual(diagnosticRedactionProblems(previewDto(), "S1", "FP1"), []);
    assert.ok(
      diagnosticRedactionProblems(previewDto({ fingerprintRedacted: "FP1-FULL" }), "S1", "FP1-FULL").length > 0,
      "full fingerprint in the preview is flagged",
    );
    assert.ok(
      diagnosticRedactionProblems(previewDto({ redactedEnvelope: '{"serial":"S1"}' }), "S1", "FP1").length > 0,
      "raw serial in the envelope is flagged",
    );
    assert.ok(
      diagnosticRedactionProblems(previewDto({ redactedEnvelope: '{"fingerprint":"FP1"}' }), "S1", "FP1").length > 0,
      "raw fingerprint in the envelope is flagged",
    );
  });

  it("validates export destinations with messages mirroring the Rust codes", () => {
    assert.ok((exportPathProblem("") ?? "").length > 0, "empty destination blocked");
    assert.equal(exportPathCode(""), "export-path-required");
    assert.ok((exportPathProblem("report.json") ?? "").length > 0, "bare file name blocked");
    assert.equal(exportPathCode("report.json"), "export-path-required");
    assert.ok((exportPathProblem("docs/out.json") ?? "").length > 0, "relative path blocked");
    assert.equal(exportPathCode("docs/out.json"), "export-path-required");
    assert.ok(
      (exportPathProblem("C:\\a\\..\\b\\out.json") ?? "").length > 0,
      "parent traversal blocked",
    );
    assert.equal(exportPathCode("C:\\a\\..\\b\\out.json"), "rejected-command-payload");
    assert.ok((exportPathProblem("C:\\exports\\") ?? "").length > 0, "folder-only destination blocked");
    assert.equal(exportPathProblem("C:\\exports\\diag.json"), null, "well-shaped absolute path passes the shape check");
    assert.equal(exportPathCode("/tmp/unscroll/diag.json"), "ok");
  });

  it("blocks the export call on a bad destination and writes only after preview", async () => {
    let calls = 0;
    const flow = new DiagnosticsFlow(
      { serial: "S1", fingerprint: "FP1" },
      {
        previewDiagnostics: async () => ok(previewDto()),
        exportDiagnostics: async () => {
          calls += 1;
          return ok('{"exported":true}');
        },
      },
    );
    await flow.export("report.json");
    assert.equal(calls, 0, "bad destination never reaches the backend");
    assert.equal(flow.snapshot.status, "failed");
    await flow.preview();
    assert.deepEqual(flow.snapshot.preview, previewDto());
    await flow.export("C:\\exports\\diag.json");
    assert.equal(calls, 1);
    assert.equal(flow.snapshot.status, "exported");
  });

  it("fails a well-shaped export closed before any preview, without a backend call", async () => {
    let calls = 0;
    const announced: string[] = [];
    const flow = new DiagnosticsFlow(
      { serial: "S1", fingerprint: "FP1" },
      {
        previewDiagnostics: async () => ok(previewDto()),
        exportDiagnostics: async () => {
          calls += 1;
          return ok('{"exported":true}');
        },
      },
      { announce: (message) => void announced.push(message) },
    );
    await flow.export("C:\\exports\\diag.json");
    assert.equal(calls, 0, "preview-first violation never reaches the backend");
    assert.equal(flow.snapshot.status, "failed");
    assert.equal(flow.snapshot.error?.code, "export-path-required");
    assert.match(flow.snapshot.error?.message ?? "", /preview/i, "guidance orders preview first");
    assert.ok(announced.length >= 1, "the block is announced, never silent");
    await flow.preview();
    await flow.export("C:\\exports\\diag.json");
    assert.equal(calls, 1, "export proceeds once the preview is loaded");
    assert.equal(flow.snapshot.status, "exported");
  });
});

describe("review round: exit stays guarded while the window is recorded open", () => {
  it("blocks exit in every still-open branch, frees it otherwise", () => {
    assert.equal(canExitMaintenance({ status: "open", opened: true, busy: false }), false);
    assert.equal(canExitMaintenance({ status: "failed", opened: true, busy: false }), false, "failed close keeps the guard");
    assert.equal(
      canExitMaintenance({ status: "recoverable-disconnect", opened: true, busy: false }),
      false,
      "disconnect keeps the guard",
    );
    assert.equal(
      canExitMaintenance({ status: "inconsistent-state", opened: true, busy: false }),
      false,
      "disagreeing records keep the guard",
    );
    assert.equal(canExitMaintenance({ status: "opening", opened: false, busy: true }), false);
    assert.equal(canExitMaintenance({ status: "closing", opened: true, busy: true }), false);
    assert.equal(canExitMaintenance(flowClosedId("closed")), true);
    assert.equal(canExitMaintenance({ status: "idle", opened: false, busy: false }), true);
    assert.equal(canExitMaintenance({ status: "failed", opened: false, busy: false }), true, "a failed open frees exit");
  });

  it("a disconnect while open retains the recorded window and the guard", async () => {
    const flow = new MaintenanceFlow(
      { serial: "S1", fingerprint: "FP1" },
      {
        discoverDevices: async () => ok({ serial: "S1" }),
        openMaintenance: async () => ok("{}"),
        closeMaintenance: async () => ok("{}"),
      },
    );
    await flow.open("OPEN STORE MAINTENANCE");
    assert.equal(flow.snapshot.opened, true);
    flow.noteRemoteProgress(JSON.stringify({ event: "disconnected", serial: "S1", detail: null }));
    assert.equal(flow.snapshot.status, "recoverable-disconnect");
    assert.equal(flow.snapshot.opened, true, "the window is still recorded open");
    assert.equal(canExitMaintenance(flow.snapshot), false, "Back stays guarded until a verified close");
  });

  function flowClosedId(status: "closed"): { status: "closed"; opened: boolean; busy: boolean } {
    return { status, opened: false, busy: false };
  }
});

describe("review round: retained states survive view re-entry", () => {
  it("keys policy flows by inspection and session, resetting only on change", () => {
    const first = policyFlowKeyFor("S1", "FP1", session("active-policy"));
    assert.equal(policyFlowKeyFor("S1", "FP1", session("active-policy")), first, "same inspection keeps its flows");
    assert.ok(
      policyFlowKeyFor("S1", "FP1", session("restore-ready")) !== first,
      "a changed session resets the flows",
    );
    assert.ok(policyFlowKeyFor("S2", "FP1", session("active-policy")) !== first, "a new phone resets the flows");
    assert.ok(policyFlowKeyFor("S1", "FP2", session("active-policy")) !== first, "a new binding resets the flows");
    assert.equal(policyFlowKeyFor(null, null, null), "\n\n", "missing identity still keys deterministically");
  });

  it("re-entry keeps a retained cleanup-retry: start never repeats restored mutations", async () => {
    let starts = 0;
    const flow = new RestoreFlow(
      { serial: "S1", fingerprint: "FP1" },
      {
        discoverDevices: async () => ok({ serial: "S1" }),
        startRestore: async () => {
          starts += 1;
          return ok(restoreJson("cleanup-retry"));
        },
        respondToDecision: async () => ok(restoreJson("complete")),
        retryCleanup: async () => ok('{"cleaned":true}'),
      },
    );
    await flow.start("RESTORE MY PHONE");
    assert.equal(flow.snapshot.status, "cleanup-retry");
    // Backing out and re-entering reuses this flow: a repeated start is a
    // no-op, so the retained retry action is the only way forward.
    await flow.start("RESTORE MY PHONE");
    assert.equal(starts, 1, "no second backend start after retention");
    assert.equal(flow.snapshot.status, "cleanup-retry");
    await flow.retry();
    assert.equal(flow.snapshot.status, "complete");
  });

  it("re-entry keeps an open window: open never runs twice", async () => {
    let opens = 0;
    const flow = new MaintenanceFlow(
      { serial: "S1", fingerprint: "FP1" },
      {
        discoverDevices: async () => ok({ serial: "S1" }),
        openMaintenance: async () => {
          opens += 1;
          return ok("{}");
        },
        closeMaintenance: async () => ok("{}"),
      },
    );
    await flow.open("OPEN STORE MAINTENANCE");
    assert.equal(flow.snapshot.status, "open");
    await flow.open("OPEN STORE MAINTENANCE");
    assert.equal(opens, 1, "no second backend open after retention");
    assert.equal(flow.snapshot.opened, true);
  });
});
