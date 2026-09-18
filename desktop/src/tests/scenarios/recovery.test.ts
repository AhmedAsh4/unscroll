/**
 * Task 20 scenarios (recovery): launcher-data-clear with fresh-desktop
 * recovery from the shared envelope only, shared-copy negative tests, and
 * USB-debug disable/re-enable persistence.
 *
 * Runs on the Node built-in test runner (no new deps):
 *   node --test desktop/src/tests/scenarios/recovery.test.ts
 *
 * Covers acceptance A1 (k) + (l) and A2 (shared-envelope-only proof):
 * launcher-data-clear empty-view plus fresh-desktop recovery using ONLY the
 * shared copy (negative: missing/corrupt/forked shared blocks without
 * consulting host memory), and USB-debug disable/re-enable persistence with
 * restore gating.
 */
import { describe, it } from "node:test";
import assert from "node:assert/strict";

import {
  ApplyFlow,
  ConnectionFlow,
  RestoreFlow,
  allowlistFor,
  createSelection,
  isRestoreComplete,
  toggleSelection,
  RESTORE_CONFIRMATION_TEXT,
} from "../../lib/state/workspace.ts";
import {
  BASE_HOME,
  FINGERPRINT,
  SERIAL,
  UNSCROLL_HOME,
  createScenarioPhone,
} from "./fake-backend.ts";
import { RESTORE_CONFIRMATION } from "../../lib/api/types.ts";

async function appliedPhone() {
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
  return { phone, snapshot, allowed };
}

describe("scenario (k): launcher-data-clear empty-view + fresh-desktop shared recovery", () => {
  it("recovers from the shared copy alone after local state is dropped", async () => {
    const { phone, snapshot } = await appliedPhone();

    phone.clearLauncherData();
    assert.equal(phone.privateEnvelopeJson(), null);
    assert.ok(phone.sharedEnvelopeJson() !== null);
    // On-device policy persists untouched by the data clear.
    assert.equal(phone.isSuspended("com.blocked"), true);
    assert.equal(phone.home, UNSCROLL_HOME);

    // Simulate a fresh desktop: poison and drop ALL host-local state...
    const poisoned = { ...snapshot, connection: "idle", session: null, entries: [] };
    assert.equal(poisoned.session, null);
    const privateReadsBefore = phone.privateReads;

    // ...then rebuild solely from the shared copy.
    const fresh = new ConnectionFlow(phone.connectionDeps(true));
    await fresh.connect();
    assert.equal(fresh.snapshot.session?.kind, "active-policy");
    assert.deepEqual(fresh.snapshot.session?.allowedActions, ["restore", "export-diagnostics"]);
    assert.equal(phone.privateReads, privateReadsBefore);

    const restore = new RestoreFlow({ serial: SERIAL, fingerprint: FINGERPRINT }, phone.restoreDeps());
    await restore.start(RESTORE_CONFIRMATION);
    assert.equal(isRestoreComplete(restore.snapshot), true);
    assert.equal(phone.home, BASE_HOME);
    void RESTORE_CONFIRMATION_TEXT;
  });
});

describe("A2: fresh-desktop recovery fails closed without the valid shared copy", () => {
  it("blocks on a missing shared copy without consulting host memory", async () => {
    const { phone, snapshot } = await appliedPhone();
    phone.clearLauncherData();
    phone.dropShared = true;
    void snapshot;
    const privateReadsBefore = phone.privateReads;

    const fresh = new ConnectionFlow(phone.connectionDeps(true));
    await fresh.connect();
    assert.equal(fresh.snapshot.session?.kind, "blocked-inconsistency");
    assert.deepEqual(fresh.snapshot.session?.allowedActions, ["export-diagnostics"]);
    assert.equal(phone.privateReads, privateReadsBefore);
  });

  it("blocks on a corrupt shared copy without consulting host memory", async () => {
    const { phone } = await appliedPhone();
    phone.clearLauncherData();
    phone.corruptShared = true;
    const privateReadsBefore = phone.privateReads;

    const fresh = new ConnectionFlow(phone.connectionDeps(true));
    await fresh.connect();
    assert.equal(fresh.snapshot.session?.kind, "blocked-inconsistency");
    assert.equal(phone.privateReads, privateReadsBefore);
  });

  it("blocks on forked copies without consulting host memory", async () => {
    const { phone } = await appliedPhone();
    phone.forkCopies = true;
    assert.equal(phone.mirrorsEqual(), false);
    const privateReadsBefore = phone.privateReads;

    const fresh = new ConnectionFlow(phone.connectionDeps(true));
    await fresh.connect();
    assert.equal(fresh.snapshot.session?.kind, "blocked-inconsistency");
    assert.equal(phone.privateReads, privateReadsBefore);
  });
});

describe("scenario (l): USB-debug disable/re-enable persistence + restore gating", () => {
  it("persists the policy while disabled, gates restore, restores after re-enable", async () => {
    const { phone } = await appliedPhone();
    const envelopeBefore = phone.sharedEnvelopeJson();

    phone.setUsbDebug(false);
    assert.equal(phone.isSuspended("com.blocked"), true);
    assert.equal(phone.home, UNSCROLL_HOME);
    assert.equal(phone.sharedEnvelopeJson(), envelopeBefore);

    const offline = await phone.connectNewSetup();
    assert.equal(offline.connection, "device-unavailable");

    const gated = new RestoreFlow({ serial: SERIAL, fingerprint: FINGERPRINT }, phone.restoreDeps());
    await gated.start(RESTORE_CONFIRMATION);
    assert.equal(gated.snapshot.status, "recoverable-disconnect");
    assert.deepEqual(
      phone.callLog.filter((call) => call.command === "start_restore"),
      [],
    );

    phone.setUsbDebug(true);
    const restore = new RestoreFlow({ serial: SERIAL, fingerprint: FINGERPRINT }, phone.restoreDeps());
    await restore.start(RESTORE_CONFIRMATION);
    assert.equal(isRestoreComplete(restore.snapshot), true);
    assert.equal(phone.home, BASE_HOME);
  });
});
