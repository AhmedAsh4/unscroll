/**
 * Task 20 scenarios (policy management): edit, maintenance, restore, and
 * cleanup retry through the REAL `workspace.ts` machines against the
 * faithful fake backend (`fake-backend.ts`, mirroring the Rust `FakeAdb`
 * outcome classes for edit, maintenance, and restore).
 *
 * Runs on the Node built-in test runner (no new deps):
 *   node --test desktop/src/tests/scenarios/policy.test.ts
 *
 * Covers acceptance A1 (i), (j), (m), (n): edit delta preserving the
 * baseline, maintenance open/warning/exit-intercept/close rescan re-block +
 * disconnect-recovery forced close, restore with the exact confirmation plus
 * ordered inverses and HOME restore verification, and cleanup retry after a
 * shared-cleanup failure.
 */
import { describe, it } from "node:test";
import assert from "node:assert/strict";

import {
  EditFlow,
  MaintenanceFlow,
  RestoreFlow,
  allowlistFor,
  canEditPolicy,
  canExitMaintenance,
  computeEditDelta,
  createSelection,
  isRestoreComplete,
  maintenanceExitBlockReason,
  toggleSelection,
  MAINTENANCE_CONFIRMATION_TEXT,
  RESTORE_CONFIRMATION_TEXT,
} from "../../lib/state/workspace.ts";
import {
  BASE_HOME,
  BASELINE_LAUNCHER,
  FINGERPRINT,
  SERIAL,
  UNSCROLL_HOME,
  createScenarioPhone,
  type FakePhone,
} from "./fake-backend.ts";
import { ApplyFlow } from "../../lib/state/workspace.ts";

async function appliedPhone(): Promise<{ phone: FakePhone; allowed: string[] }> {
  const phone = createScenarioPhone();
  const snapshot = await phone.connectNewSetup();
  const selection = toggleSelection(
    createSelection(snapshot.entries),
    snapshot.entries.find((row) => row.packageId === "com.keep")!,
  );
  const allowed = allowlistFor(snapshot.entries, selection);
  const apply = new ApplyFlow({ serial: SERIAL, fingerprint: FINGERPRINT, allowed, entries: snapshot.entries }, phone.applyDeps());
  await apply.start();
  assert.equal(apply.snapshot.status, "complete");
  return { phone, allowed };
}

describe("scenario (i): edit delta preserves the baseline", () => {
  it("applies only the delta and keeps the baseline launcher and hash", async () => {
    const { phone, allowed } = await appliedPhone();
    const session = phone.sessionResponse(false);
    assert.equal(session.kind, "active-policy");
    assert.equal(canEditPolicy("recovery-required", session), true);

    const next = [...allowed, "com.blocked"];
    const delta = computeEditDelta(allowed, next);
    assert.deepEqual(delta, { addedToKeep: ["com.blocked"], addedToBlock: [] });

    const snapshot = await phone.connectNewSetup();
    const edit = new EditFlow({ serial: SERIAL, fingerprint: FINGERPRINT, allowed: next, entries: snapshot.entries }, phone.editDeps());
    await edit.start();
    assert.equal(edit.snapshot.status, "complete");
    assert.equal(phone.isSuspended("com.blocked"), false);
    assert.equal(phone.privateEnvelope?.baselineLauncher, BASELINE_LAUNCHER);
    assert.equal(phone.privateEnvelope?.baselineHash, "baseline-hash-test");
    assert.equal(phone.mirrorsEqual(), true);
  });

  it("surfaces a rejected plan with its guidance and never retries silently", async () => {
    const { phone, allowed } = await appliedPhone();
    phone.rejectEditPlan = true;
    const snapshot = await phone.connectNewSetup();
    const edit = new EditFlow({ serial: SERIAL, fingerprint: FINGERPRINT, allowed, entries: snapshot.entries }, phone.editDeps());
    await edit.start();
    assert.equal(edit.snapshot.status, "failed");
    assert.equal(edit.snapshot.error?.code, "plan-rejected");
  });
});

describe("scenario (j): store maintenance open, exit intercept, close, disconnect", () => {
  it("opens on the exact phrase, blocks exit, re-blocks new apps on close", async () => {
    const { phone } = await appliedPhone();
    const maintenance = new MaintenanceFlow({ serial: SERIAL, fingerprint: FINGERPRINT }, phone.maintenanceDeps());
    await maintenance.open(MAINTENANCE_CONFIRMATION_TEXT);
    assert.equal(maintenance.snapshot.status, "open");
    assert.equal(maintenance.snapshot.opened, true);
    assert.equal(canExitMaintenance(maintenance.snapshot), false);
    assert.ok((maintenanceExitBlockReason(maintenance.snapshot) ?? "").length > 0);
    assert.equal(phone.sessionKind(), "maintenance-recovery");

    // A mismatched phrase never reaches the backend.
    const callsBefore = phone.callLog.filter((call) => call.command === "open_maintenance").length;
    const mismatch = new MaintenanceFlow({ serial: SERIAL, fingerprint: FINGERPRINT }, phone.maintenanceDeps());
    await mismatch.open("OPEN MAINTENANCE");
    assert.equal(mismatch.snapshot.status, "failed");
    assert.equal(phone.callLog.filter((call) => call.command === "open_maintenance").length, callsBefore);

    // A newly installed unapproved app is re-blocked by the verified close.
    phone.newlyInstalled = ["com.newapp"];
    phone.entries.push({
      packageId: "com.newapp",
      label: "New App",
      suspended: false,
      enabled: true,
      protected: false,
      protectedReason: null,
      iconCached: false,
      isStore: false,
      isInstallSource: false,
    });
    const approved = phone.entries.filter((row) => !row.suspended).map((row) => row.packageId).filter((id) => id !== "com.newapp");
    const scanned = phone.entries.map((row) => row.packageId);
    await maintenance.close(approved, scanned);
    assert.equal(maintenance.snapshot.status, "closed");
    assert.equal(canExitMaintenance(maintenance.snapshot), true);
    assert.equal(phone.isSuspended("com.newapp"), true);
    assert.equal(phone.mirrorsEqual(), true);
  });

  it("keeps the window recorded open across a disconnect so close is forced", async () => {
    const { phone } = await appliedPhone();
    const maintenance = new MaintenanceFlow({ serial: SERIAL, fingerprint: FINGERPRINT }, phone.maintenanceDeps());
    await maintenance.open(MAINTENANCE_CONFIRMATION_TEXT);
    phone.disconnectMutate = true;
    await maintenance.close(["com.keep"], ["com.keep"]);
    assert.equal(maintenance.snapshot.status, "recoverable-disconnect");
    assert.equal(maintenance.snapshot.opened, true);
    assert.equal(canExitMaintenance(maintenance.snapshot), false);

    phone.disconnectMutate = false;
    await maintenance.close(["com.keep"], ["com.keep"]);
    assert.equal(maintenance.snapshot.status, "closed");
  });
});

describe("scenario (m): restore with confirmation, ordered inverses, HOME verification", () => {
  it("blocks a mismatched phrase without calling the backend, restores in order on exact", async () => {
    const { phone } = await appliedPhone();
    const restore = new RestoreFlow({ serial: SERIAL, fingerprint: FINGERPRINT }, phone.restoreDeps());
    await restore.start("RESTORE");
    assert.equal(restore.snapshot.status, "failed");
    assert.deepEqual(
      phone.callLog.filter((call) => call.command === "start_restore"),
      [],
    );

    await restore.start(RESTORE_CONFIRMATION_TEXT);
    assert.equal(isRestoreComplete(restore.snapshot), true);
    assert.deepEqual(phone.inverseLog, [
      "inverse app_op allow com.source",
      "inverse unsuspend blocked",
      `inverse home ${BASE_HOME}`,
    ]);
    assert.equal(phone.home, BASE_HOME);
    assert.ok(phone.homeCalls.some((call) => call.includes(BASE_HOME)));
    assert.equal(phone.isSuspended("com.blocked"), false);
    assert.equal(phone.isSuspended("com.store"), false);
  });

  it("answers the restore HOME chooser: cancel waits, confirm restores", async () => {
    const { phone } = await appliedPhone();
    phone.failHomeRestore = true;
    const restore = new RestoreFlow({ serial: SERIAL, fingerprint: FINGERPRINT }, phone.restoreDeps());
    await restore.start(RESTORE_CONFIRMATION_TEXT);
    assert.equal(restore.snapshot.status, "chooser-required");

    await restore.answerChooser("home-cancelled");
    assert.equal(restore.snapshot.status, "chooser-required");

    phone.failHomeRestore = false;
    await restore.answerChooser("home-confirmed");
    assert.equal(isRestoreComplete(restore.snapshot), true);
    assert.equal(phone.home, BASE_HOME);
    void UNSCROLL_HOME;
  });
});

describe("scenario (n): cleanup retry after a shared-cleanup failure", () => {
  it("retains cleanup-retry, fails a retry while shared cleanup fails, completes after", async () => {
    const { phone } = await appliedPhone();
    phone.failCleanupShared = true;
    const restore = new RestoreFlow({ serial: SERIAL, fingerprint: FINGERPRINT }, phone.restoreDeps());
    await restore.start(RESTORE_CONFIRMATION_TEXT);
    assert.equal(restore.snapshot.status, "cleanup-retry");
    assert.equal(phone.home, BASE_HOME);

    await restore.retry();
    assert.equal(restore.snapshot.status, "failed");
    assert.ok(phone.sharedEnvelopeJson() !== null);

    phone.failCleanupShared = false;
    const fresh = new RestoreFlow({ serial: SERIAL, fingerprint: FINGERPRINT }, phone.restoreDeps());
    phone.cleanupRemaining = true;
    assert.equal(phone.sessionKind(), "cleanup-retry");
    await fresh.start(RESTORE_CONFIRMATION_TEXT);
    assert.equal(isRestoreComplete(fresh.snapshot), true);
  });
});
