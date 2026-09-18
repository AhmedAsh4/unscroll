/**
 * Task 20 scenarios (install -> apply): whole desktop workflow through the
 * REAL `workspace.ts` machines against the faithful fake backend
 * (`fake-backend.ts`, which mirrors the Rust `FakeAdb` outcome classes).
 *
 * Runs on the Node built-in test runner (no new deps):
 *   node --test desktop/src/tests/scenarios/workflow.test.ts
 *
 * Covers acceptance A1 (a)-(h) + (o) and A3 (no-mutation proof):
 * install/bootstrap+inspect, real-icon catalog with missing-icon fallback,
 * allowlist-first select/review with protected locks, apply success incl.
 * HOME shell path, interruption/resume + disconnect recovery,
 * ordinary-failure Continue vs Rollback pausing before HOME, store-failure
 * hard rollback with no override, HOME chooser fallback + cancellation, and
 * optional app-op partial protection honestly labeled.
 */
import { describe, it } from "node:test";
import assert from "node:assert/strict";

import {
  ApplyFlow,
  allowlistFor,
  blockingAppFor,
  canApply,
  createIconLoader,
  createSelection,
  fallbackInitial,
  filterApps,
  isApplyComplete,
  pillForEntry,
  resolveIconSrc,
  reviewGroups,
  rolledBackRecoveryText,
  selectRowIcon,
  toggleSelection,
} from "../../lib/state/workspace.ts";
import {
  BASE_HOME,
  FINGERPRINT,
  FIRST_JOURNAL_ID,
  SERIAL,
  SHARED_RECOVERY_PATH,
  UNSCROLL_HOME,
  createScenarioPhone,
} from "./fake-backend.ts";

describe("scenario (a): install/bootstrap + inspect", () => {
  it("inspects a bootstrapped phone with entries and a new-setup session", async () => {
    const phone = createScenarioPhone();
    assert.equal(phone.bootstrapInstalled, true);
    const snapshot = await phone.connectNewSetup();
    assert.equal(snapshot.connection, "ready");
    assert.ok(snapshot.entries.length > 0);
    assert.equal(snapshot.session?.kind, "new-setup");
    assert.equal(snapshot.device?.fingerprint, FINGERPRINT);
    assert.equal(SHARED_RECOVERY_PATH, "/sdcard/Documents/Unscroll/recovery-v1.json");
  });
});

describe("scenario (b): real-icon catalog display incl. missing-icon fallback", () => {
  it("resolves cached icons, falls back on typed misses, never guesses brand art", async () => {
    const phone = createScenarioPhone();
    const snapshot = await phone.connectNewSetup();
    assert.ok(snapshot.entries.some((entry) => entry.iconCached));

    const loader = createIconLoader(phone.iconTransport);
    const cached = await loader.get("com.keep");
    assert.ok(typeof cached === "string" && cached.startsWith("data:image/png;base64,"));
    assert.equal(resolveIconSrc(cached), cached);

    const miss = await loader.get("com.blocked");
    assert.equal(miss, null);
    assert.equal(resolveIconSrc({ missing: true }), null);
    assert.equal(fallbackInitial("Blocked App"), "B");
    assert.equal(fallbackInitial(""), "?");

    assert.equal(resolveIconSrc("http://example.com/icon.png"), null);
    assert.equal(resolveIconSrc("data:image/svg+xml;base64,AAAA"), null);
    assert.equal(resolveIconSrc("not-a-url"), null);
    assert.equal(resolveIconSrc(null), null);
  });

  it("gives fresh-catalog precedence: uncached entries render fallback despite stale hits", async () => {
    const phone = createScenarioPhone();
    const loader = createIconLoader(phone.iconTransport);
    await loader.get("com.keep");
    const stale = loader.cached("com.keep");
    const snapshot = await phone.connectNewSetup();
    const entry = snapshot.entries.find((row) => row.packageId === "com.keep")!;
    assert.ok(entry);
    // A same-packageId entry that is no longer cached must not reuse a stale hit.
    assert.deepEqual(selectRowIcon({ ...entry, iconCached: false }, stale), {
      kind: "value",
      src: null,
    });
  });
});

describe("scenario (c): select/review allowlist-first with protected locks", () => {
  it("keeps protected locked, blocks everything else by default, reconciles groups", async () => {
    const phone = createScenarioPhone();
    const snapshot = await phone.connectNewSetup();
    const mutatingBefore = phone.mutatingCalls.length;

    const selection = createSelection(snapshot.entries);
    assert.equal(selection["com.base"], true);
    assert.equal(selection["com.system"], true);
    assert.equal(selection["com.keep"], false);
    assert.equal(selection["com.blocked"], false);

    const relocked = toggleSelection(selection, snapshot.entries.find((row) => row.packageId === "com.base")!);
    assert.equal(relocked["com.base"], true);
    const kept = toggleSelection(selection, snapshot.entries.find((row) => row.packageId === "com.keep")!);
    assert.equal(kept["com.keep"], true);

    const groups = reviewGroups(snapshot.entries, kept);
    const total =
      groups.kept.length + groups.blocked.length + groups.stores.length + groups.protected.length + groups.unsupported.length;
    assert.equal(total, snapshot.entries.length);
    assert.equal(groups.baselineLauncher?.packageId, "com.base");
    assert.ok(groups.unsupported.some((row) => row.packageId === "com.ambiguous"));

    assert.equal(pillForEntry(snapshot.entries.find((row) => row.packageId === "com.base")!, true), "protected");
    assert.equal(pillForEntry(snapshot.entries.find((row) => row.packageId === "com.ambiguous")!, true), "unsupported");
    assert.equal(
      pillForEntry(snapshot.entries.find((row) => row.packageId === "com.store")!, false),
      "store",
    );

    assert.ok(filterApps(snapshot.entries, kept, "keep", "all").some((row) => row.packageId === "com.keep"));
    assert.equal(canApply({ connection: snapshot.connection, session: snapshot.session, entries: snapshot.entries }), true);
    assert.deepEqual(allowlistFor(snapshot.entries, kept).includes("com.keep"), true);

    // A3 no-mutation proof: choose/review (select, filter, group, icons)
    // issue zero mutating invokes.
    const loader = createIconLoader(phone.iconTransport);
    await loader.get("com.keep");
    await loader.get("com.blocked");
    assert.equal(phone.mutatingCalls.length, mutatingBefore);
  });
});

describe("scenario (d): apply success incl. HOME shell path", () => {
  it("mirrors pending before exec, suspends blocked, verifies HOME, mirrors applied", async () => {
    const phone = createScenarioPhone();
    const snapshot = await phone.connectNewSetup();
    const selection = toggleSelection(
      createSelection(snapshot.entries),
      snapshot.entries.find((row) => row.packageId === "com.keep")!,
    );
    const allowed = allowlistFor(snapshot.entries, selection);
    const flow = new ApplyFlow(
      { serial: SERIAL, fingerprint: FINGERPRINT, allowed, entries: snapshot.entries },
      phone.applyDeps(),
    );
    await flow.start();
    assert.equal(isApplyComplete(flow.snapshot), true);
    assert.equal(phone.home, UNSCROLL_HOME);
    assert.ok(phone.homeCalls.some((call) => call.includes(UNSCROLL_HOME)));
    assert.equal(phone.isSuspended("com.blocked"), true);
    assert.equal(phone.isSuspended("com.keep"), false);
    assert.equal(phone.isSuspended("com.base"), false);
    assert.equal(phone.mirrorsEqual(), true);
    assert.ok(phone.mirrorLog.length > 0);
    assert.equal(phone.mirrorLog[0][0], phone.mirrorLog[0][1]);
    assert.ok((phone.mirrorLog[0][0] ?? "").includes('"state":"pending"'));
  });
});

describe("scenario (e): interruption/resume + disconnect recovery", () => {
  it("retains mirrored pending on disconnect and resumes without a duplicate id", async () => {
    const phone = createScenarioPhone();
    phone.disconnectMutate = true;
    const snapshot = await phone.connectNewSetup();
    const selection = toggleSelection(
      createSelection(snapshot.entries),
      snapshot.entries.find((row) => row.packageId === "com.keep")!,
    );
    const input = { serial: SERIAL, fingerprint: FINGERPRINT, allowed: allowlistFor(snapshot.entries, selection), entries: snapshot.entries };
    const interrupted = new ApplyFlow(input, phone.applyDeps());
    await interrupted.start();
    assert.equal(interrupted.snapshot.status, "recoverable-disconnect");
    assert.equal(phone.mirrorsEqual(), true);
    assert.ok((phone.privateEnvelopeJson() ?? "").includes('"state":"pending"'));
    assert.equal(phone.sessionKind(), "resumable-transaction");

    phone.disconnectMutate = false;
    const resumed = new ApplyFlow(input, phone.applyDeps());
    await resumed.start();
    assert.equal(isApplyComplete(resumed.snapshot), true);
    assert.equal(phone.home, UNSCROLL_HOME);
    const envelope = phone.privateEnvelopeJson() ?? "";
    assert.equal(envelope.split(FIRST_JOURNAL_ID).length - 1, 1);
  });
});

describe("scenario (f): ordinary failure pauses before HOME with Continue vs Rollback", () => {
  it("continues after the pause and records the failed step honestly", async () => {
    const phone = createScenarioPhone();
    phone.failVerifyPackage = "com.blocked";
    const snapshot = await phone.connectNewSetup();
    const selection = toggleSelection(
      createSelection(snapshot.entries),
      snapshot.entries.find((row) => row.packageId === "com.keep")!,
    );
    const input = { serial: SERIAL, fingerprint: FINGERPRINT, allowed: allowlistFor(snapshot.entries, selection), entries: snapshot.entries };
    const flow = new ApplyFlow(input, phone.applyDeps());
    await flow.start();
    assert.equal(flow.snapshot.status, "decision-required");
    assert.equal(flow.snapshot.blockingPackage, "com.blocked");
    assert.deepEqual(blockingAppFor(snapshot.entries, flow.snapshot.blockingPackage), {
      packageId: "com.blocked",
      label: "Blocked App",
    });
    assert.equal(phone.home, BASE_HOME);
    assert.deepEqual(phone.homeCalls, []);

    phone.failVerifyPackage = null;
    await flow.answerDecision("continue");
    assert.equal(isApplyComplete(flow.snapshot), true);
    const envelope = phone.privateEnvelopeJson() ?? "";
    assert.ok(envelope.includes("failed") && envelope.includes("launcher_policy"));
    assert.equal(phone.home, UNSCROLL_HOME);
  });

  it("rolls back to unchanged suspensions and HOME on user rollback", async () => {
    const phone = createScenarioPhone();
    phone.failVerifyPackage = "com.blocked";
    const snapshot = await phone.connectNewSetup();
    const selection = toggleSelection(
      createSelection(snapshot.entries),
      snapshot.entries.find((row) => row.packageId === "com.keep")!,
    );
    const flow = new ApplyFlow(
      { serial: SERIAL, fingerprint: FINGERPRINT, allowed: allowlistFor(snapshot.entries, selection), entries: snapshot.entries },
      phone.applyDeps(),
    );
    await flow.start();
    assert.equal(flow.snapshot.status, "decision-required");
    await flow.answerDecision("rollback");
    assert.equal(flow.snapshot.status, "rolled-back");
    assert.equal(phone.isSuspended("com.blocked"), false);
    assert.equal(phone.home, BASE_HOME);
    assert.deepEqual(phone.homeCalls, []);
    assert.equal(phone.mirrorsEqual(), true);
  });
});

describe("scenario (g): store failure is a hard rollback with no override", () => {
  it("rolls back, never touches HOME, and offers no continue path", async () => {
    const phone = createScenarioPhone();
    phone.failMutatePackage = "com.store";
    phone.failVerifyPackage = "com.store";
    const snapshot = await phone.connectNewSetup();
    const selection = toggleSelection(
      createSelection(snapshot.entries),
      snapshot.entries.find((row) => row.packageId === "com.keep")!,
    );
    const flow = new ApplyFlow(
      { serial: SERIAL, fingerprint: FINGERPRINT, allowed: allowlistFor(snapshot.entries, selection), entries: snapshot.entries },
      phone.applyDeps(),
    );
    await flow.start();
    assert.equal(flow.snapshot.status, "rolled-back");
    assert.equal(flow.snapshot.rolledBackFrom, "direct");
    assert.deepEqual(phone.homeCalls, []);
    assert.equal(phone.home, BASE_HOME);
    assert.equal(phone.isSuspended("com.blocked"), false);
    assert.equal(phone.isSuspended("com.store"), false);
    assert.equal(phone.mirrorsEqual(), true);
    // No reduced-protection override: a terminal rollback answers nothing.
    await flow.answerDecision("continue");
    assert.equal(flow.snapshot.status, "rolled-back");
    assert.equal(rolledBackRecoveryText(flow.snapshot), "All completed changes were rolled back and verified; the phone is unchanged. Review the selection and try again.");
  });
});

describe("scenario (h): HOME chooser fallback + cancellation", () => {
  it("re-enters the chooser on cancel with the same pending id, then completes", async () => {
    const phone = createScenarioPhone();
    phone.failHome = true;
    const snapshot = await phone.connectNewSetup();
    const selection = toggleSelection(
      createSelection(snapshot.entries),
      snapshot.entries.find((row) => row.packageId === "com.keep")!,
    );
    const flow = new ApplyFlow(
      { serial: SERIAL, fingerprint: FINGERPRINT, allowed: allowlistFor(snapshot.entries, selection), entries: snapshot.entries },
      phone.applyDeps(),
    );
    await flow.start();
    assert.equal(flow.snapshot.status, "chooser-required");
    const callsBefore = phone.homeCalls.length;

    await flow.answerChooser("home-cancelled");
    assert.equal(flow.snapshot.status, "chooser-required");
    assert.equal(phone.homeCalls.length, callsBefore);
    assert.equal(phone.home, BASE_HOME);
    assert.ok((phone.privateEnvelopeJson() ?? "").includes(FIRST_JOURNAL_ID));

    phone.failHome = false;
    await flow.answerChooser("home-confirmed");
    assert.equal(isApplyComplete(flow.snapshot), true);
    assert.equal(phone.home, UNSCROLL_HOME);
    const envelope = phone.privateEnvelopeJson() ?? "";
    assert.equal(envelope.split(FIRST_JOURNAL_ID).length - 1, 1);
  });
});

describe("scenario (o): optional app-op failure is honest partial protection", () => {
  it("completes with the gap labeled and announced", async () => {
    const phone = createScenarioPhone();
    phone.failAppOp = true;
    phone.falseAppOp = true;
    const snapshot = await phone.connectNewSetup();
    const selection = toggleSelection(
      createSelection(snapshot.entries),
      snapshot.entries.find((row) => row.packageId === "com.keep")!,
    );
    const flow = new ApplyFlow(
      { serial: SERIAL, fingerprint: FINGERPRINT, allowed: allowlistFor(snapshot.entries, selection), entries: snapshot.entries },
      phone.applyDeps(),
    );
    await flow.start();
    assert.equal(isApplyComplete(flow.snapshot), true);
    assert.equal(flow.snapshot.partialProtection.length, 1);
    assert.match(flow.snapshot.announcement, /partial/i);
    assert.equal(phone.home, UNSCROLL_HOME);
    assert.equal(phone.mirrorsEqual(), true);
  });
});
