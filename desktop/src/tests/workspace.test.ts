/**
 * Focused failing tests for Task 16: workspace.ts connection-state machine.
 *
 * Runs on the Node built-in test runner (no added dependencies):
 *   node --test src/tests/workspace.test.ts
 *
 * Covers: connection-state mapping for every stable error code, step-state
 * derivation, unverified-newer-API detection, MIUI guidance detection,
 * session-kind routing, robust inspect parsing (both Rust shapes), bare
 * string|object progress payloads (Task 15 known issue 1), fail-closed
 * guidance surfacing (Task 15 known issue 2), and stale-device /
 * busy-transaction / device-replacement guards.
 */
import { describe, it } from "node:test";
import assert from "node:assert/strict";

import {
  STEPS,
  SHOW_BUSY_AFTER_MS,
  PROGRESS_NAMES,
  connectionFromCode,
  guidanceForError,
  isUnverifiedNewer,
  isMiuiGuidance,
  parseInspectResult,
  normalizeProgressPayload,
  deriveSteps,
  routeForSession,
  ConnectionFlow,
  type CommandError,
  type ConnectionState,
} from "../lib/state/workspace.ts";

function err(code: CommandError["code"], message = "bridge message", action = "bridge action"): CommandError {
  return { code, message, action };
}

describe("connectionFromCode maps every stable error code", () => {
  const cases: Array<[CommandError["code"], ConnectionState]> = [
    ["no-device", "no-device"],
    ["multiple-devices", "multiple-devices"],
    ["unauthorized-device", "unauthorized-device"],
    ["device-unavailable", "device-unavailable"],
    ["missing-driver", "missing-driver"],
    ["unsupported-device", "unsupported-device"],
    ["preflight-failed", "preflight-failed"],
    ["recovery-required", "recovery-required"],
    ["incompatible-launcher", "incompatible-launcher"],
    ["stale-device", "stale-device"],
    ["busy-transaction", "busy-transaction"],
    // Device-replacement guards: untrusted identity inputs.
    ["invalid-serial", "device-replaced"],
    ["invalid-fingerprint", "device-replaced"],
    // Fail-closed Task 15 handlers surface reconcile/close guidance, never silent retry.
    ["plan-rejected", "recovery-required"],
    ["maintenance-blocked", "maintenance-recovery"],
    ["restore-blocked", "recovery-required"],
    // Non-connection codes degrade to blocking preflight guidance, never crash.
    ["invalid-package-id", "preflight-failed"],
    ["invalid-destination", "preflight-failed"],
    ["invalid-confirmation", "preflight-failed"],
    ["invalid-decision", "preflight-failed"],
    ["export-path-required", "preflight-failed"],
    ["rejected-command-payload", "preflight-failed"],
    ["icon-too-large", "preflight-failed"],
    ["icon-cache-full", "preflight-failed"],
    ["export-failed", "preflight-failed"],
  ];
  for (const [code, expected] of cases) {
    it(`${code} -> ${expected}`, () => {
      assert.equal(connectionFromCode(code), expected);
    });
  }

  it("unsupported-device mentioning newer/unverified becomes a warning, not an error", () => {
    const state = connectionFromCode("unsupported-device");
    assert.equal(state, "unsupported-device");
    const warned = guidanceForError(
      err("unsupported-device", "Android 15 is newer and unverified for Unscroll", "wait"),
    );
    assert.equal(warned.state, "unverified-newer");
    assert.equal(warned.tone, "warning");
    assert.match(warned.body, /unverified/i);
  });

  it("preflight payloads mentioning newer/unverified are warnings too", () => {
    const guided = guidanceForError(err("preflight-failed", "unverified newer API level", "wait"));
    assert.equal(guided.state, "unverified-newer");
    assert.equal(guided.tone, "warning");
  });
});

describe("guidanceForError renders human guidance plus an adjacent recovery action", () => {
  it("every connection state has heading, body, and action text", async () => {
    const { CONNECTION_COPY } = await import("../lib/state/workspace.ts");
    const states: ConnectionState[] = [
      "idle", "checking", "inspecting", "connected", "ready",
      "no-device", "unauthorized-device", "device-unavailable",
      "multiple-devices", "missing-driver", "unsupported-device",
      "unverified-newer", "incompatible-launcher", "recovery-required",
      "maintenance-recovery", "preflight-failed",
      "stale-device", "busy-transaction", "device-replaced",
    ];
    for (const state of states) {
      const copy = CONNECTION_COPY[state];
      assert.ok(copy, `missing copy for ${state}`);
      assert.ok(copy.heading.length > 0, `${state} heading`);
      assert.ok(copy.body.length > 0, `${state} body`);
      assert.ok(copy.action.length > 0, `${state} action`);
    }
  });

  it("key states name the recovery without command details", () => {
    assert.match(guidanceForError(err("no-device")).body, /USB debugging/i);
    assert.match(guidanceForError(err("unauthorized-device")).body, /authoriz/i);
    assert.match(guidanceForError(err("multiple-devices")).body, /exactly one/i);
    assert.match(guidanceForError(err("missing-driver")).body, /driver/i);
    assert.match(guidanceForError(err("unsupported-device")).body, /Android 7.*16|7-16|7 through/i);
    assert.match(guidanceForError(err("device-unavailable")).body, /cable/i);
    assert.match(guidanceForError(err("stale-device")).body, /reconnect/i);
    assert.match(guidanceForError(err("busy-transaction")).body, /wait|still running/i);
    assert.match(guidanceForError(err("incompatible-launcher")).body, /matching.*release/i);
  });

  it("never leaks commands, exit codes, or serials in static copy", async () => {
    const { CONNECTION_COPY } = await import("../lib/state/workspace.ts");
    const banned = [/adb shell/i, /pm (suspend|unsuspend|enable)/i, /exit code/i, /serial [A-Z0-9]{4,}/];
    for (const [state, copy] of Object.entries(CONNECTION_COPY)) {
      for (const field of [copy.heading, copy.body, copy.action]) {
        for (const pattern of banned) {
          assert.doesNotMatch(field, pattern, `${state} leaks technical detail`);
        }
      }
    }
  });

  it("fail-closed codes preserve the server message and action", () => {
    const guided = guidanceForError(err("maintenance-blocked", "Store safety facts unavailable.", "Reconnect the phone."));
    assert.equal(guided.state, "maintenance-recovery");
    assert.match(guided.body, /Store safety facts unavailable/);
    assert.match(guided.body, /close.*before|Close maintenance/i);
  });
});

describe("isUnverifiedNewer / isMiuiGuidance detectors", () => {
  it("detects newer/unverified API payloads", () => {
    assert.equal(isUnverifiedNewer("API 37 is newer and unverified"), true);
    assert.equal(isUnverifiedNewer("unverified release"), true);
    assert.equal(isUnverifiedNewer("ordinary preflight failure"), false);
    assert.equal(isUnverifiedNewer(""), false);
  });

  it("detects MIUI/HyperOS bootstrap guidance signals", () => {
    assert.equal(isMiuiGuidance("INSTALL_FAILED_USER_RESTRICTED during install"), true);
    assert.equal(isMiuiGuidance("SecurityException from package installer"), true);
    assert.equal(isMiuiGuidance("Enable Install via USB first"), true);
    assert.equal(isMiuiGuidance("ordinary cable failure"), false);
  });

  it("MIUI preflight failures carry Install-via-USB guidance with no bypass", () => {
    const guided = guidanceForError(err("preflight-failed", "INSTALL_FAILED_USER_RESTRICTED on HyperOS", "retry"));
    assert.equal(guided.state, "preflight-failed");
    assert.match(guided.body, /Install via USB/i);
    assert.match(guided.body, /Mi account|manufacturer/i);
    assert.doesNotMatch(guided.body, /bypass/i);
  });
});

describe("parseInspectResult accepts both Rust shapes", () => {
  const entry = {
    packageId: "com.example.app",
    label: "Example",
    suspended: false,
    enabled: true,
    protected: false,
    protectedReason: null,
    iconCached: false,
  };

  it("parses the Rust object shape {serial,model,manufacturer,api,entries}", () => {
    const parsed = parseInspectResult({
      serial: "SERIAL1",
      model: "Pixel 8",
      manufacturer: "Google",
      api: 34,
      entries: [entry],
    });
    assert.equal(parsed.device.model, "Pixel 8");
    assert.equal(parsed.device.manufacturer, "Google");
    assert.equal(parsed.device.api, 34);
    assert.equal(parsed.entries.length, 1);
    assert.equal(parsed.entries[0]?.packageId, "com.example.app");
  });

  it("parses the legacy array shape with unknown device facts", () => {
    const parsed = parseInspectResult([entry]);
    assert.equal(parsed.device.model, null);
    assert.equal(parsed.entries.length, 1);
  });

  it("drops malformed entries and degrades garbage to empty", () => {
    const parsed = parseInspectResult({ serial: "S", entries: [entry, { nope: 1 }, null, "x"] });
    assert.equal(parsed.entries.length, 1);
    assert.deepEqual(parseInspectResult(null).entries, []);
    assert.deepEqual(parseInspectResult({}).entries, []);
    assert.deepEqual(parseInspectResult("oops").entries, []);
  });
});

describe("normalizeProgressPayload accepts bare strings and objects (Task 15 issue 1)", () => {
  it("accepts every stable progress name as a bare string", () => {
    for (const name of PROGRESS_NAMES) {
      assert.equal(normalizeProgressPayload(name), name);
    }
  });

  it("accepts {event} objects and one Tauri wrapping level", () => {
    assert.equal(normalizeProgressPayload({ event: "completed", serial: "s", detail: null }), "completed");
    assert.equal(normalizeProgressPayload({ payload: "disconnected" }), "disconnected");
    assert.equal(normalizeProgressPayload({ payload: { event: "started" } }), "started");
  });

  it("rejects unknown payloads without throwing", () => {
    assert.equal(normalizeProgressPayload("bogus"), null);
    assert.equal(normalizeProgressPayload(null), null);
    assert.equal(normalizeProgressPayload(42), null);
    assert.equal(normalizeProgressPayload({}), null);
    assert.equal(normalizeProgressPayload({ event: "bogus" }), null);
  });
});

describe("deriveSteps covers the four named steps", () => {
  it("exposes Connect, Choose apps, Review, Apply in order", () => {
    assert.deepEqual(STEPS.map((s) => s.label), ["Connect", "Choose apps", "Review", "Apply"]);
    const steps = deriveSteps("idle", null);
    assert.equal(steps.length, 4);
    assert.equal(steps[0]?.status, "active");
  });

  it("marks Connect complete and Choose active once ready", () => {
    const steps = deriveSteps("ready", { kind: "new-setup", guidance: "", allowedActions: ["begin-setup"] });
    assert.equal(steps[0]?.status, "complete");
    assert.equal(steps[1]?.status, "active");
    assert.equal(steps[2]?.status, "pending");
    assert.equal(steps[3]?.status, "pending");
  });

  it("blocks later steps for recovery-required with routing intact", () => {
    const steps = deriveSteps("recovery-required", null);
    assert.equal(steps[0]?.status, "complete");
    assert.equal(steps[1]?.status, "blocked");
    assert.equal(steps[2]?.status, "blocked");
    assert.equal(steps[3]?.status, "blocked");
  });

  it("blocks later steps while a maintenance session must close first", () => {
    const steps = deriveSteps("maintenance-recovery", {
      kind: "maintenance-recovery",
      guidance: "",
      allowedActions: ["export-diagnostics"],
    });
    assert.equal(steps[1]?.status, "blocked");
  });

  it("keeps error states on Connect with retry available", () => {
    const steps = deriveSteps("no-device", null);
    assert.equal(steps[0]?.status, "active");
    assert.equal(steps[1]?.status, "pending");
  });
});

describe("routeForSession offers session-kind routing text", () => {
  it("routes every session kind with title and body", async () => {
    const kinds = [
      "new-setup", "active-policy", "maintenance-recovery", "resumable-transaction",
      "rollback-only", "restore-ready", "cleanup-retry", "blocked-inconsistency",
    ] as const;
    for (const kind of kinds) {
      const route = routeForSession(kind);
      assert.ok(route.title.length > 0, `${kind} title`);
      assert.ok(route.body.length > 0, `${kind} body`);
    }
  });

  it("recovery kinds route to edit/maintenance/restore instead of a new setup", () => {
    assert.match(routeForSession("active-policy").body, /edit|maintenance|restore/i);
    assert.match(routeForSession("restore-ready").body, /restore/i);
  });

  it("maintenance-recovery must close before other actions", () => {
    assert.match(routeForSession("maintenance-recovery").body, /close.*before|must close/i);
  });

  it("blocked inconsistency offers diagnostics only, with no command detail", () => {
    const route = routeForSession("blocked-inconsistency");
    assert.match(route.body, /diagnostic/i);
    assert.doesNotMatch(route.body, /adb/i);
  });
});

describe("ConnectionFlow state machine", () => {
  const entry = {
    packageId: "com.example.app",
    label: "Example",
    suspended: false,
    enabled: true,
    protected: false,
    protectedReason: null,
    iconCached: false,
  };

  function happyDeps() {
    return {
      discoverDevices: async () => ({ ok: true as const, value: { serial: "S1" } }),
      inspectDevice: async (_serial: string) => ({
        ok: true as const,
        value: {
          serial: "S1",
          model: "Pixel 8",
          manufacturer: "Google",
          api: 34,
          fingerprint: "fp-1",
          entries: [entry],
        },
      }),
      getSession: async (_s: string, _f: string) => ({
        ok: true as const,
        value: { kind: "new-setup" as const, guidance: "Begin setup.", allowedActions: ["begin-setup" as const] },
      }),
    };
  }

  it("runs discover -> inspect -> session and ends ready with one announcement", async () => {
    const announced: string[] = [];
    const flow = new ConnectionFlow(happyDeps(), { announce: (m) => void announced.push(m) });
    await flow.connect();
    assert.equal(flow.snapshot.connection, "ready");
    assert.equal(flow.snapshot.entries.length, 1);
    assert.equal(flow.snapshot.device?.model, "Pixel 8");
    assert.equal(flow.snapshot.session?.kind, "new-setup");
    assert.equal(flow.snapshot.busy, false);
    assert.ok(announced.length >= 1);
    assert.equal(new Set(announced).size, announced.length, "each announcement fires once");
  });

  it("maps discovery errors to connection states with recovery actions", async () => {
    const announced: string[] = [];
    const flow = new ConnectionFlow(
      {
        discoverDevices: async () => ({ ok: false as const, error: err("no-device") }),
        inspectDevice: happyDeps().inspectDevice,
        getSession: happyDeps().getSession,
      },
      { announce: (m) => void announced.push(m) },
    );
    await flow.connect();
    assert.equal(flow.snapshot.connection, "no-device");
    assert.ok(flow.snapshot.guidance.action.length > 0);
    assert.match(announced.at(-1) ?? "", /No phone|no device/i);
  });

  it("guards device replacement: mismatched inspect serial becomes stale-device", async () => {
    const deps = happyDeps();
    const flow = new ConnectionFlow({
      ...deps,
      inspectDevice: async () => ({
        ok: true as const,
        value: { serial: "OTHER", model: "M", manufacturer: "X", api: 33, entries: [] },
      }),
    });
    await flow.connect();
    assert.equal(flow.snapshot.connection, "stale-device");
  });

  it("ignores a second concurrent connect (busy-transaction guard)", async () => {
    let calls = 0;
    const deps = happyDeps();
    const flow = new ConnectionFlow({
      ...deps,
      discoverDevices: async () => {
        calls += 1;
        await new Promise((r) => setTimeout(r, 20));
        return { ok: true as const, value: { serial: "S1" } };
      },
    });
    await Promise.all([flow.connect(), flow.connect()]);
    assert.equal(calls, 1);
  });

  it("uses the 300ms persistent-feedback threshold constant", () => {
    assert.equal(SHOW_BUSY_AFTER_MS, 300);
  });

  it("surfaces the server message for preflight-failed (no unseen guidance)", () => {
    const guided = guidanceForError(err("preflight-failed", "Camera check did not pass", "retry"));
    assert.equal(guided.state, "preflight-failed");
    assert.match(guided.body, /Camera check did not pass/);
  });

  it("fails on remote disconnect while busy (cable-pull window)", () => {
    const flow = new ConnectionFlow(happyDeps());
    flow.snapshot = { ...flow.snapshot, connection: "checking", busy: true };
    flow.noteRemoteProgress("disconnected");
    assert.equal(flow.snapshot.connection, "device-unavailable");
    assert.equal(flow.snapshot.busy, false);
  });

  it("ignores remote disconnect while idle", () => {
    const flow = new ConnectionFlow(happyDeps());
    assert.equal(flow.snapshot.connection, "idle");
    flow.noteRemoteProgress("disconnected");
    assert.equal(flow.snapshot.connection, "idle");
  });

  it("routes non-new-setup sessions to recovery-required (Task 16 MINOR-5)", async () => {
    const deps = happyDeps();
    const flow = new ConnectionFlow({
      ...deps,
      getSession: async () => ({
        ok: true as const,
        value: { kind: "active-policy" as const, guidance: "Reconcile.", allowedActions: ["resume" as const] },
      }),
    });
    await flow.connect();
    assert.equal(flow.snapshot.connection, "recovery-required");
    assert.match(flow.snapshot.guidance.action, /Reconcile/i);
  });

  it("keeps maintenance-recovery sessions on the maintenance route", async () => {
    const deps = happyDeps();
    const flow = new ConnectionFlow({
      ...deps,
      getSession: async () => ({
        ok: true as const,
        value: {
          kind: "maintenance-recovery" as const,
          guidance: "Close maintenance.",
          allowedActions: ["export-diagnostics" as const],
        },
      }),
    });
    await flow.connect();
    assert.equal(flow.snapshot.connection, "maintenance-recovery");
  });
});
