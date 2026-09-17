/**
 * Task 19 structure test: owned files exist and hold the structural
 * acceptance items.
 *
 * Runs on the Node built-in test runner (no added dependencies):
 *   node --test src/tests/task19-structure.test.ts
 *
 * Guards: owned screens/components exist, the workspace Task 19 section
 * appends the four flows plus gating helpers, typed confirmations stay
 * exact, maintenance guards exit, restore keeps retry and chooser
 * fallback, blocked histories keep diagnostics only, previews render
 * every DTO field, announcements travel through the shell region, and
 * forbidden strings stay absent (no manual-change buttons, no device
 * request construction, no hard-coded art, no fresh-setup offers).
 */
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const desktop = join(here, "..", "..");

function read(rel: string): string {
  const full = join(desktop, rel);
  assert.ok(existsSync(full), `owned file exists: ${rel}`);
  return readFileSync(full, "utf8");
}

const NEW_SCREENS = [
  "src/lib/screens/ActivePolicyScreen.svelte",
  "src/lib/screens/MaintenanceScreen.svelte",
  "src/lib/screens/RestoreScreen.svelte",
  "src/lib/screens/DiagnosticsScreen.svelte",
];
const NEW_COMPONENTS = [
  "src/lib/components/TypedConfirmation.svelte",
  "src/lib/components/MaintenanceWarning.svelte",
  "src/lib/components/RestoreSummary.svelte",
  "src/lib/components/DiagnosticPreview.svelte",
  "src/lib/components/RecoveryBlock.svelte",
];
const NEW_ALL = [...NEW_SCREENS, ...NEW_COMPONENTS];

function task19Section(): string {
  const source = read("src/lib/state/workspace.ts");
  const marker = "Task 19: active-policy, maintenance, restore, and diagnostics.";
  const index = source.indexOf(marker);
  assert.ok(index !== -1, "workspace.ts carries the Task 19 section marker");
  return source.slice(index);
}

describe("owned files exist", () => {
  for (const rel of NEW_ALL) {
    it(rel, () => {
      assert.ok(existsSync(join(desktop, rel)), `${rel} exists`);
    });
  }

  it("workspace Task 19 section appends the flows and gating helpers", () => {
    const section = task19Section();
    for (const name of [
      "EditFlow",
      "MaintenanceFlow",
      "RestoreFlow",
      "DiagnosticsFlow",
      "isPolicyReconciled",
      "canEditPolicy",
      "canRestorePolicy",
      "policyGateReason",
      "policyActionsFor",
      "policyFlowKeyFor",
      "safeRecoveryActions",
      "requiresCloseFirst",
      "computeEditDelta",
      "isMaintenanceConfirmation",
      "isRestoreConfirmation",
      "parseRestoreOutcome",
      "parseDiagnosticPreview",
      "diagnosticRedactionProblems",
      "exportPathProblem",
      "exportPathCode",
      "canExitMaintenance",
      "maintenanceExitBlockReason",
    ]) {
      assert.ok(section.includes(name), `Task 19 section exports ${name}`);
    }
  });
});

describe("no manual-change buttons and no request construction", () => {
  it("owned presentation never offers a manual-change button", () => {
    for (const rel of NEW_ALL) {
      assert.doesNotMatch(read(rel), /force/i, `${rel} offers no manual-change button`);
      assert.doesNotMatch(read(rel), /override/i, `${rel} offers no manual-change button`);
    }
  });

  it("Task 19 state section never offers one either", () => {
    assert.doesNotMatch(task19Section(), /force/i, "Task 19 section offers no manual-change button");
    assert.doesNotMatch(task19Section(), /override/i, "Task 19 section offers no manual-change button");
  });

  it("owned sources reach Rust only through invoke helpers plus DTO types", () => {
    const combined = [...NEW_ALL, "src/App.svelte"].map(read).join("\n");
    assert.doesNotMatch(combined, /from\s+["']@tauri-apps\/api\/core["']/, "no direct invoke imports");
    assert.doesNotMatch(combined, /child_process|execSync/, "no process spawning");
    assert.doesNotMatch(combined, /spawn\(/, "no process spawning");
    assert.doesNotMatch(combined, /adb\s+shell/i, "no device requests");
    assert.doesNotMatch(combined, /pm\s+(suspend|unsuspend)/i, "no package requests");
  });

  it("owned presentation carries no hard-coded art", () => {
    for (const rel of NEW_ALL) {
      assert.doesNotMatch(read(rel), /data:image/, `${rel} carries no hard-coded art`);
      assert.doesNotMatch(read(rel), /https?:\/\//, `${rel} carries no remote art`);
    }
  });

  it("owned presentation never offers a fresh setup", () => {
    for (const rel of NEW_ALL) {
      assert.doesNotMatch(read(rel), /begin-setup/, `${rel} never offers a fresh setup`);
      assert.doesNotMatch(read(rel), /start a new baseline/i, `${rel} never offers a fresh baseline`);
      assert.doesNotMatch(read(rel), /create a new baseline/i, `${rel} never offers a fresh baseline`);
    }
  });

  it("owned styles add no decorative motion", () => {
    const combined = NEW_ALL.map(read).join("\n");
    assert.doesNotMatch(combined, /@keyframes/, "no keyframe animation");
    assert.doesNotMatch(combined, /animation:/, "no animation declarations");
  });
});

describe("single live region (no nested announcements)", () => {
  it("new presentation adds no live regions or status roles", () => {
    for (const rel of NEW_ALL) {
      assert.doesNotMatch(read(rel), /aria-live/, `${rel} must not add a live region`);
      assert.doesNotMatch(read(rel), /role="status"/, `${rel} must not add a status role`);
      assert.doesNotMatch(read(rel), /role="alert"/, `${rel} must not add an alert role`);
    }
  });

  it("every new screen announces through the shell prop callback", () => {
    for (const rel of NEW_SCREENS) {
      assert.ok(read(rel).includes("onAnnounce"), `${rel} announces via the shell region`);
    }
  });

  it("busy states reuse the 300ms persistent-status marker", () => {
    for (const rel of NEW_SCREENS) {
      assert.ok(read(rel).includes("SHOW_BUSY_AFTER_MS"), `${rel} reuses the shared marker`);
    }
  });
});

describe("typed confirmations stay exact", () => {
  it("TypedConfirmation compares verbatim and disables on mismatch", () => {
    const source = read("src/lib/components/TypedConfirmation.svelte");
    assert.match(source, /value !== expected/, "verbatim mismatch check present");
    assert.ok(source.includes("disabled"), "mismatch disables the confirm control");
    assert.ok(source.includes("focus()"), "confirmation moves focus to the input");
  });

  it("maintenance gates on the exact warning acknowledgment", () => {
    const combined =
      read("src/lib/components/MaintenanceWarning.svelte") + read("src/lib/screens/MaintenanceScreen.svelte");
    assert.ok(combined.includes("OPEN STORE MAINTENANCE"), "exact maintenance phrase present");
    assert.ok(combined.includes("MaintenanceWarning"), "screen renders the warning component");
    assert.match(combined, /new install/i, "new-install exposure is stated");
  });

  it("restore gates on the exact restore phrase", () => {
    const source = read("src/lib/screens/RestoreScreen.svelte");
    assert.ok(source.includes("RESTORE MY PHONE"), "exact restore phrase present");
    assert.ok(source.includes("TypedConfirmation"), "screen renders the typed confirmation");
  });
});

describe("maintenance guards ordinary exit until the verified close", () => {
  it("screen surfaces the open state and the exit guard", () => {
    const source = read("src/lib/screens/MaintenanceScreen.svelte");
    assert.ok(source.includes("maintenanceExitBlockReason"), "exit guard reason is shown");
    assert.ok(source.includes("Close maintenance"), "verified close control present");
    assert.ok(source.includes("listenProgress"), "screen owns its progress subscription");
    assert.ok(source.includes("decodeProgressPayload"), "screen decodes JSON-text payloads");
    assert.ok(source.includes("normalizeProgressPayload"), "screen narrows via the workspace normalizer");
  });

  it("review round: every branch renders the guard and Back stays blocked while open", () => {
    const source = read("src/lib/screens/MaintenanceScreen.svelte");
    assert.ok(source.includes("canExitMaintenance"), "Back gating helper is wired");
    assert.ok(source.includes("exitBlocked"), "Back derives from the exit guard");
    const rendered = source.match(/\{exitReason\}/g) ?? [];
    assert.ok(rendered.length >= 5, `exit reason renders in every branch (found ${rendered.length})`);
    assert.ok(source.includes("disabled={snap.busy || exitBlocked}"), "Back stays blocked while the window is recorded open");
    const closes = source.match(/Close maintenance/g) ?? [];
    assert.ok(closes.length >= 2, "failed and interrupted branches keep a close retry, not just Back");
  });
});

describe("restore keeps summary, chooser fallback, and cleanup retry", () => {
  it("screen reuses the summary, progress, and chooser guidance", () => {
    const source = read("src/lib/screens/RestoreScreen.svelte");
    assert.ok(source.includes("RestoreSummary"), "recorded-changes summary present");
    assert.ok(source.includes("OperationProgress"), "inverse progress present");
    assert.ok(source.includes("LauncherChooserGuide"), "chooser fallback present");
    assert.match(source, /Retry cleanup/, "cleanup retry retained");
  });

  it("summary lists the recorded changes", () => {
    const source = read("src/lib/components/RestoreSummary.svelte");
    assert.ok(source.includes("changes"), "recorded-changes list present");
  });
});

describe("blocked histories keep diagnostics only", () => {
  it("RecoveryBlock guides to diagnostics with no other action", () => {
    const source = read("src/lib/components/RecoveryBlock.svelte");
    assert.match(source, /diagnostic/i, "diagnostic entry present");
    assert.ok(source.includes("onExportDiagnostics"), "diagnostics entry is wired");
  });

  it("ActivePolicyScreen renders the block for inconsistent histories", () => {
    const source = read("src/lib/screens/ActivePolicyScreen.svelte");
    assert.ok(source.includes("RecoveryBlock"), "block is rendered");
    assert.ok(source.includes("blocked-inconsistency"), "inconsistent histories route to the block");
  });
});

describe("diagnostics preview every DTO field before the destination", () => {
  it("DiagnosticPreview renders all preview fields", () => {
    const source = read("src/lib/components/DiagnosticPreview.svelte");
    for (const field of [
      "deviceModel",
      "fingerprintRedacted",
      "allowlistCount",
      "initialPackageCount",
      "operations",
      "errors",
      "warnings",
      "redactedEnvelope",
    ]) {
      assert.ok(source.includes(field), `preview renders ${field}`);
    }
  });

  it("DiagnosticsScreen previews first and writes locally only", () => {
    const source = read("src/lib/screens/DiagnosticsScreen.svelte");
    assert.ok(source.includes("DiagnosticPreview"), "preview rendered before the destination");
    assert.ok(source.includes("flow.preview"), "preview action present");
    assert.ok(source.includes("flow.export"), "local export action present");
    assert.match(source, /locally only|on this computer/i, "local-only write is stated");
  });
});

describe("policy routing reuses the chooser concepts and preserves the baseline", () => {
  it("ActivePolicyScreen mirrors the chooser semantics", () => {
    const source = read("src/lib/screens/ActivePolicyScreen.svelte");
    assert.ok(source.includes("filterApps"), "search/filter semantics reused");
    assert.ok(source.includes("selectionCounts"), "count semantics reused");
    assert.ok(source.includes("isPolicyReconciled"), "reconciliation gate present");
    assert.ok(source.includes("policyGateReason"), "gate reason present");
    assert.match(source, /baseline/i, "baseline note present");
    assert.match(source, /preserv/i, "baseline preservation is stated");
    assert.ok(source.includes("addedToKeep") && source.includes("addedToBlock"), "delta-only summary present");
  });
});

describe("App wires the session-routed views", () => {
  it("App renders the policy, maintenance, restore, and diagnostics views", () => {
    const source = read("src/App.svelte");
    for (const name of ["ActivePolicyScreen", "MaintenanceScreen", "RestoreScreen", "DiagnosticsScreen"]) {
      assert.ok(source.includes(name), `App wires ${name}`);
    }
    assert.ok(source.includes("isPolicyReconciled"), "App routes reconciled sessions to policy options");
    assert.ok(source.includes("Continue to policy options"), "policy entry control present");
  });

  it("review round: App preserves retained flows and guards maintenance Back", () => {
    const source = read("src/App.svelte");
    assert.ok(source.includes("if (!maintenanceFlow)"), "maintenance flow survives view re-entry");
    assert.ok(source.includes("if (!restoreFlow)"), "restore flow survives view re-entry");
    assert.ok(source.includes("policyFlowKeyFor"), "flows reset only on a new inspection or session");
    assert.ok(source.includes("policyFlowKey"), "flow identity is tracked");
    assert.ok(source.includes("canExitMaintenance"), "maintenance Back is guarded in the host");
  });
});
