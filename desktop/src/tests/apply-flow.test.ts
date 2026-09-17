/**
 * Task 18 RED: pure apply-state machine in workspace.ts.
 *
 * Runs on the Node built-in test runner (no added dependencies):
 *   node --test src/tests/apply-flow.test.ts
 *
 * Covers the Task 18 acceptance checklist via the pure machine:
 * success, slow-progress busy marker, ordinary continue/rollback,
 * store hard failure (rollback, no override), optional partial warning,
 * disconnect, resume-relevant outcomes, HOME chooser + cancel,
 * rollback-failure/inconsistent, completion gating (complete-only),
 * no-double-start, stale-device/discover guards, and no
 * frontend-only completion (progress "completed" alone never completes).
 */
import { describe, it } from "node:test";
import assert from "node:assert/strict";

import {
  APPLY_OUTCOMES,
  ROLLED_BACK_GENERIC_LEDE,
  ROLLED_BACK_GENERIC_RECOVERY,
  SHOW_BUSY_AFTER_MS,
  allowlistFor,
  blockingAppFor,
  extractProgressDetail,
  isApplyComplete,
  isStoreRollback,
  parseApplyOutcome,
  rolledBackLedeText,
  rolledBackRecoveryText,
  ApplyFlow,
  type ApplyDeps,
  type ApplyInvokeResult,
} from "../lib/state/workspace.ts";
import type { AppEntryDto, CommandError } from "../lib/api/types.ts";

function entry(packageId: string, label: string): AppEntryDto {
  return {
    packageId,
    label,
    suspended: false,
    enabled: true,
    protected: false,
    protectedReason: null,
    iconCached: false,
    isStore: false,
    isInstallSource: false,
  };
}

function ok(value: unknown): ApplyInvokeResult {
  return { ok: true, value };
}

function fail(code: CommandError["code"]): ApplyInvokeResult {
  return { ok: false, error: { code, message: `test ${code}`, action: "Try again." } };
}

function outcomeJson(outcome: string, pkg: string | null = null, partial: string[] = []): string {
  return JSON.stringify({ outcome, package: pkg, partialProtection: partial });
}

const ENTRIES = [entry("com.example.keep", "Keep"), entry("com.example.maps", "Maps")];

function makeFlow(deps: ApplyDeps, announced: string[] = []): ApplyFlow {
  return new ApplyFlow(
    { serial: "S1", fingerprint: "FP1", allowed: ["com.example.keep"], entries: ENTRIES },
    deps,
    { announce: (message) => void announced.push(message) },
  );
}

function happyDeps(overrides: Partial<ApplyDeps> = {}): ApplyDeps {
  return {
    discoverDevices: async () => ok({ serial: "S1" }),
    startApply: async () => ok(outcomeJson("complete")),
    respondToDecision: async () => ok(outcomeJson("complete")),
    ...overrides,
  };
}

describe("parseApplyOutcome reads the backend JSON envelope", () => {
  it("parses a stringified outcome with package and partial protection", () => {
    const parsed = parseApplyOutcome(outcomeJson("decision-required", "com.example.maps", ["Sideload path"]));
    assert.deepEqual(parsed, {
      outcome: "decision-required",
      package: "com.example.maps",
      partialProtection: ["Sideload path"],
    });
  });

  it("parses an already-structured object payload", () => {
    const parsed = parseApplyOutcome({ outcome: "complete", package: null, partialProtection: [] });
    assert.deepEqual(parsed, { outcome: "complete", package: null, partialProtection: [] });
  });

  it("rejects garbage, unknown outcomes, and non-objects without throwing", () => {
    assert.equal(parseApplyOutcome(null), null);
    assert.equal(parseApplyOutcome("{{{not json"), null);
    assert.equal(parseApplyOutcome(JSON.stringify({ outcome: "bogus", package: null, partialProtection: [] })), null);
    assert.equal(parseApplyOutcome({ outcome: "complete", package: null, partialProtection: "x" }), null);
    assert.equal(parseApplyOutcome(42), null);
  });

  it("exposes every terminal backend outcome name", () => {
    assert.deepEqual([...APPLY_OUTCOMES].sort(), [
      "chooser-required",
      "complete",
      "decision-required",
      "inconsistent-state",
      "recoverable-disconnect",
      "rolled-back",
    ]);
  });
});

describe("allowlistFor and blockingAppFor derive presentation data", () => {
  it("keeps only selected packageIds as the allowlist", () => {
    const entries = [entry("a", "A"), entry("b", "B"), entry("p", "P")];
    assert.deepEqual(allowlistFor(entries, { a: true, b: false, p: true }), ["a", "p"]);
  });

  it("names the blocking app by label and packageId", () => {
    const entries = [entry("com.example.maps", "Maps")];
    assert.deepEqual(blockingAppFor(entries, "com.example.maps"), {
      packageId: "com.example.maps",
      label: "Maps",
    });
    assert.equal(blockingAppFor(entries, "com.example.unknown"), null);
    assert.equal(blockingAppFor(entries, null), null);
  });

  it("extracts the decision detail from JSON-text progress payloads", () => {
    const raw = JSON.stringify({ event: "decision-required", serial: "S1", detail: "com.example.maps" });
    assert.equal(extractProgressDetail(raw), "com.example.maps");
    assert.equal(extractProgressDetail({ event: "started", serial: null, detail: null }), null);
    assert.equal(extractProgressDetail("completed"), null);
  });
});

describe("ApplyFlow success and busy marker", () => {
  it("completes only on the backend complete outcome", async () => {
    const announced: string[] = [];
    const flow = makeFlow(happyDeps(), announced);
    await flow.start();
    assert.equal(flow.snapshot.status, "complete");
    assert.equal(flow.snapshot.outcome, "complete");
    assert.equal(flow.snapshot.busy, false);
    assert.equal(isApplyComplete(flow.snapshot), true);
    assert.ok(announced.length >= 1);
    assert.equal(new Set(announced).size, announced.length, "each announcement fires once");
  });

  it("marks busy while the live re-check is in flight (300ms persistence owner)", async () => {
    const flow = makeFlow(happyDeps());
    const pending = flow.start();
    assert.equal(flow.snapshot.busy, true, "busy synchronously before the first await resolves");
    await pending;
    assert.equal(flow.snapshot.busy, false);
    assert.equal(SHOW_BUSY_AFTER_MS, 300);
  });

  it("counts verified operation progress without completing early", async () => {
    const flow = makeFlow(happyDeps({ startApply: async () => ok(outcomeJson("chooser-required")) }));
    await flow.start();
    flow.noteRemoteProgress(JSON.stringify({ event: "operation-applied", serial: "S1", detail: null }));
    flow.noteRemoteProgress("operation-applied");
    assert.equal(flow.snapshot.appliedCount, 2);
    assert.equal(isApplyComplete(flow.snapshot), false);
  });
});

describe("ordinary-app failure offers continue or rollback before HOME", () => {
  it("pauses with the blocking package named, then continues to complete", async () => {
    const seen: string[] = [];
    const flow = makeFlow(
      happyDeps({
        startApply: async () => ok(outcomeJson("decision-required", "com.example.maps")),
        respondToDecision: async (_s, _f, decision) => {
          seen.push(decision);
          return ok(outcomeJson("complete"));
        },
      }),
    );
    await flow.start();
    assert.equal(flow.snapshot.status, "decision-required");
    assert.equal(flow.snapshot.blockingPackage, "com.example.maps");
    assert.equal(isApplyComplete(flow.snapshot), false);
    await flow.answerDecision("continue");
    assert.deepEqual(seen, ["continue"]);
    assert.equal(flow.snapshot.status, "complete");
    assert.equal(isApplyComplete(flow.snapshot), true);
  });

  it("rolls back on the rollback answer and never completes", async () => {
    const seen: string[] = [];
    const flow = makeFlow(
      happyDeps({
        startApply: async () => ok(outcomeJson("decision-required", "com.example.maps")),
        respondToDecision: async (_s, _f, decision) => {
          seen.push(decision);
          return ok(outcomeJson("rolled-back"));
        },
      }),
    );
    await flow.start();
    await flow.answerDecision("rollback");
    assert.deepEqual(seen, ["rollback"]);
    assert.equal(flow.snapshot.status, "rolled-back");
    assert.equal(isApplyComplete(flow.snapshot), false);
  });
});

describe("store hard failure rolls back with no override", () => {
  it("lands rolled-back directly with no decision call", async () => {
    let decisions = 0;
    const flow = makeFlow(
      happyDeps({
        startApply: async () => ok(outcomeJson("rolled-back")),
        respondToDecision: async () => {
          decisions += 1;
          return ok(outcomeJson("rolled-back"));
        },
      }),
    );
    await flow.start();
    assert.equal(flow.snapshot.status, "rolled-back");
    assert.equal(flow.snapshot.outcome, "rolled-back");
    assert.equal(isApplyComplete(flow.snapshot), false);
    assert.equal(decisions, 0);
  });
});

describe("optional sideload gaps stay partial, never blocked", () => {
  it("completes with partial protection preserved verbatim", async () => {
    const flow = makeFlow(happyDeps({ startApply: async () => ok(outcomeJson("complete", null, ["Sideload path A"])) }));
    await flow.start();
    assert.equal(flow.snapshot.status, "complete");
    assert.deepEqual(flow.snapshot.partialProtection, ["Sideload path A"]);
    assert.equal(isApplyComplete(flow.snapshot), true);
  });
});

describe("disconnect invalidates the flow", () => {
  it("maps a JSON-text disconnect event to recoverable-disconnect", async () => {
    const flow = makeFlow(happyDeps({ startApply: async () => ok(outcomeJson("decision-required", "com.example.maps")) }));
    await flow.start();
    assert.equal(flow.snapshot.status, "decision-required");
    flow.noteRemoteProgress(JSON.stringify({ event: "disconnected", serial: "S1", detail: null }));
    assert.equal(flow.snapshot.status, "recoverable-disconnect");
    assert.equal(flow.snapshot.busy, false);
    assert.equal(isApplyComplete(flow.snapshot), false);
  });

  it("maps the backend recoverable-disconnect outcome", async () => {
    const flow = makeFlow(happyDeps({ startApply: async () => ok(outcomeJson("recoverable-disconnect")) }));
    await flow.start();
    assert.equal(flow.snapshot.status, "recoverable-disconnect");
    assert.equal(isApplyComplete(flow.snapshot), false);
  });

  it("ignores later decisions once disconnected", async () => {
    let calls = 0;
    const flow = makeFlow(
      happyDeps({
        startApply: async () => ok(outcomeJson("decision-required", "com.example.maps")),
        respondToDecision: async () => {
          calls += 1;
          return ok(outcomeJson("complete"));
        },
      }),
    );
    await flow.start();
    flow.noteRemoteProgress("disconnected");
    await flow.answerDecision("continue");
    assert.equal(calls, 0);
    assert.equal(flow.snapshot.status, "recoverable-disconnect");
  });
});

describe("HOME chooser fallback waits for the user, verifies via backend", () => {
  it("confirms the chooser only through the backend complete outcome", async () => {
    const seen: string[] = [];
    const flow = makeFlow(
      happyDeps({
        startApply: async () => ok(outcomeJson("chooser-required")),
        respondToDecision: async (_s, _f, decision) => {
          seen.push(decision);
          return ok(outcomeJson("complete"));
        },
      }),
    );
    await flow.start();
    assert.equal(flow.snapshot.status, "chooser-required");
    assert.equal(isApplyComplete(flow.snapshot), false);
    await flow.answerChooser("home-confirmed");
    assert.deepEqual(seen, ["home-confirmed"]);
    assert.equal(flow.snapshot.status, "complete");
    assert.equal(isApplyComplete(flow.snapshot), true);
  });

  it("cancelling the chooser rolls back without completing", async () => {
    const flow = makeFlow(
      happyDeps({
        startApply: async () => ok(outcomeJson("chooser-required")),
        respondToDecision: async () => ok(outcomeJson("rolled-back")),
      }),
    );
    await flow.start();
    await flow.answerChooser("home-cancelled");
    assert.equal(flow.snapshot.status, "rolled-back");
    assert.equal(isApplyComplete(flow.snapshot), false);
  });
});

describe("rollback failure and inconsistent state stay blocked", () => {
  it("maps the inconsistent-state outcome and progress event", async () => {
    const flow = makeFlow(happyDeps({ startApply: async () => ok(outcomeJson("inconsistent-state")) }));
    await flow.start();
    assert.equal(flow.snapshot.status, "inconsistent-state");
    assert.equal(isApplyComplete(flow.snapshot), false);

    const flow2 = makeFlow(happyDeps({ startApply: async () => ok(outcomeJson("decision-required", "com.example.maps")) }));
    await flow2.start();
    flow2.noteRemoteProgress("inconsistent-state");
    assert.equal(flow2.snapshot.status, "inconsistent-state");
  });

  it("tracks rollback progress events without completing", () => {
    const flow = makeFlow(happyDeps());
    flow.noteRemoteProgress("rollback-started");
    flow.noteRemoteProgress("rollback-applied");
    assert.equal(flow.snapshot.rollbackCount, 1);
    assert.equal(isApplyComplete(flow.snapshot), false);
  });
});

describe("completion gating and frontend-only completion", () => {
  it("is complete only for the backend complete outcome", async () => {
    for (const outcome of ["decision-required", "chooser-required", "rolled-back", "recoverable-disconnect", "inconsistent-state"] as const) {
      const flow = makeFlow(happyDeps({ startApply: async () => ok(outcomeJson(outcome, "com.example.maps")) }));
      await flow.start();
      assert.equal(isApplyComplete(flow.snapshot), false, `${outcome} must not complete`);
    }
  });

  it("never completes on progress completed alone", () => {
    const flow = makeFlow(happyDeps());
    flow.noteRemoteProgress("completed");
    flow.noteRemoteProgress(JSON.stringify({ event: "completed", serial: "S1", detail: null }));
    assert.equal(isApplyComplete(flow.snapshot), false);
    assert.equal(flow.snapshot.status, "idle");
  });
});

describe("guards: no double-start and live-device re-check", () => {
  it("collapses concurrent starts into one backend call", async () => {
    let discoveries = 0;
    let applies = 0;
    const flow = makeFlow(
      happyDeps({
        discoverDevices: async () => {
          discoveries += 1;
          await new Promise((r) => setTimeout(r, 20));
          return ok({ serial: "S1" });
        },
        startApply: async () => {
          applies += 1;
          return ok(outcomeJson("complete"));
        },
      }),
    );
    await Promise.all([flow.start(), flow.start()]);
    assert.equal(discoveries, 1);
    assert.equal(applies, 1);
  });

  it("never mutates on a replaced device (serial mismatch fails closed)", async () => {
    let applies = 0;
    const flow = makeFlow(
      happyDeps({
        discoverDevices: async () => ok({ serial: "OTHER" }),
        startApply: async () => {
          applies += 1;
          return ok(outcomeJson("complete"));
        },
      }),
    );
    await flow.start();
    assert.equal(applies, 0);
    assert.equal(flow.snapshot.status, "failed");
    assert.equal(flow.snapshot.error?.code, "stale-device");
    assert.equal(isApplyComplete(flow.snapshot), false);
  });

  it("fails to recoverable-disconnect when discovery reports the cable dropped", async () => {
    let applies = 0;
    const flow = makeFlow(
      happyDeps({
        discoverDevices: async () => fail("device-unavailable"),
        startApply: async () => {
          applies += 1;
          return ok(outcomeJson("complete"));
        },
      }),
    );
    await flow.start();
    assert.equal(applies, 0);
    assert.equal(flow.snapshot.status, "recoverable-disconnect");
  });

  it("answers are no-ops outside their decision state", async () => {
    let calls = 0;
    const flow = makeFlow(
      happyDeps({
        respondToDecision: async () => {
          calls += 1;
          return ok(outcomeJson("complete"));
        },
      }),
    );
    await flow.answerDecision("continue");
    await flow.answerChooser("home-confirmed");
    assert.equal(calls, 0);
  });
});

describe("rolled-back origin tracks a known store cause only for direct rollbacks (F1)", () => {
  it("marks an ordinary user rollback after decision-required as user-rollback with generic copy", async () => {
    const flow = makeFlow(
      happyDeps({
        startApply: async () => ok(outcomeJson("decision-required", "com.example.maps")),
        respondToDecision: async () => ok(outcomeJson("rolled-back")),
      }),
    );
    await flow.start();
    await flow.answerDecision("rollback");
    assert.equal(flow.snapshot.status, "rolled-back");
    assert.equal(flow.snapshot.rolledBackFrom, "user-rollback");
    assert.equal(isStoreRollback(flow.snapshot), false);
    assert.equal(rolledBackRecoveryText(flow.snapshot), ROLLED_BACK_GENERIC_RECOVERY);
    assert.equal(rolledBackLedeText(flow.snapshot), ROLLED_BACK_GENERIC_LEDE);
    assert.doesNotMatch(rolledBackRecoveryText(flow.snapshot) ?? "", /incomplete store protection/i);
    assert.doesNotMatch(rolledBackLedeText(flow.snapshot) ?? "", /incomplete store protection/i);
  });

  it("marks a chooser cancel as chooser-cancel with generic copy", async () => {
    const flow = makeFlow(
      happyDeps({
        startApply: async () => ok(outcomeJson("chooser-required")),
        respondToDecision: async () => ok(outcomeJson("rolled-back")),
      }),
    );
    await flow.start();
    await flow.answerChooser("home-cancelled");
    assert.equal(flow.snapshot.status, "rolled-back");
    assert.equal(flow.snapshot.rolledBackFrom, "chooser-cancel");
    assert.equal(isStoreRollback(flow.snapshot), false);
    assert.equal(rolledBackRecoveryText(flow.snapshot), ROLLED_BACK_GENERIC_RECOVERY);
    assert.equal(rolledBackLedeText(flow.snapshot), ROLLED_BACK_GENERIC_LEDE);
  });

  it("marks a direct backend rolled-back as direct with the generic copy", async () => {
    const flow = makeFlow(happyDeps({ startApply: async () => ok(outcomeJson("rolled-back")) }));
    await flow.start();
    assert.equal(flow.snapshot.status, "rolled-back");
    assert.equal(flow.snapshot.rolledBackFrom, "direct");
    assert.equal(rolledBackRecoveryText(flow.snapshot), ROLLED_BACK_GENERIC_RECOVERY);
    assert.equal(rolledBackLedeText(flow.snapshot), ROLLED_BACK_GENERIC_LEDE);
    assert.doesNotMatch(rolledBackRecoveryText(flow.snapshot) ?? "", /incomplete store protection/i);
    assert.doesNotMatch(rolledBackLedeText(flow.snapshot) ?? "", /incomplete store protection/i);
  });

  it("ordinary-rollback copy matches the direct-rollback copy (generic everywhere)", async () => {
    const ordinary = makeFlow(
      happyDeps({
        startApply: async () => ok(outcomeJson("decision-required", "com.example.maps")),
        respondToDecision: async () => ok(outcomeJson("rolled-back")),
      }),
    );
    await ordinary.start();
    await ordinary.answerDecision("rollback");
    const direct = makeFlow(happyDeps({ startApply: async () => ok(outcomeJson("rolled-back")) }));
    await direct.start();
    assert.equal(rolledBackLedeText(ordinary.snapshot), rolledBackLedeText(direct.snapshot));
    assert.equal(rolledBackRecoveryText(ordinary.snapshot), rolledBackRecoveryText(direct.snapshot));
  });
});

describe("progress announcements never repeat on count-only events (F2)", () => {
  it("N consecutive operation-applied events do not grow the announcement call count", async () => {
    const announced: string[] = [];
    const flow = makeFlow(
      happyDeps({ startApply: async () => ok(outcomeJson("decision-required", "com.example.maps")) }),
      announced,
    );
    await flow.start();
    const before = announced.length;
    for (let i = 0; i < 5; i += 1) {
      flow.noteRemoteProgress("operation-applied");
    }
    assert.equal(flow.snapshot.appliedCount, 5);
    assert.equal(announced.length, before);
  });

  it("rollback-applied progress does not announce either", () => {
    const announced: string[] = [];
    const flow = makeFlow(happyDeps(), announced);
    const before = announced.length;
    flow.noteRemoteProgress("rollback-started");
    const afterStart = announced.length;
    assert.ok(afterStart >= before);
    for (let i = 0; i < 3; i += 1) {
      flow.noteRemoteProgress("rollback-applied");
    }
    assert.equal(flow.snapshot.rollbackCount, 3);
    assert.equal(announced.length, afterStart);
  });
});
