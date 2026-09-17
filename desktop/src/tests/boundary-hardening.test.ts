/**
 * RED (TDD) for the pre-Task-18 boundary-hardening pass.
 *
 * Runs on the Node built-in test runner (no added dependencies):
 *   node --test src/tests/boundary-hardening.test.ts
 *
 * Covers:
 * - FIX 1: events.ts string payloads are JSON.parsed (fall back to
 *   bare-name) before normalizeProgressPayload; garbage stays null.
 * - FIX 2: icon loader cache-dedup + fallback-on-miss logic.
 * - FIX 3: store flags in the catalog DTO; reviewGroups prefers flags and
 *   keeps the whole-token heuristic only for legacy entries without flags.
 */
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

import { decodeProgressPayload, listenProgress } from "../lib/api/events.ts";
import {
  createIconLoader,
  normalizeProgressPayload,
  parseInspectResult,
  pillForEntry,
  resolveIconSrc,
  reviewGroups,
  selectRowIcon,
} from "../lib/state/workspace.ts";
import type { AppEntryDto } from "../lib/api/types.ts";

function flagged(overrides: Partial<AppEntryDto> & { packageId: string; label: string }): AppEntryDto {
  return {
    suspended: false,
    enabled: true,
    protected: false,
    protectedReason: null,
    iconCached: false,
    isStore: false,
    isInstallSource: false,
    ...overrides,
  };
}

describe("FIX 1: structured progress strings decode before normalizing", () => {
  it("decodes a stringified object payload to its event name", () => {
    const raw = JSON.stringify({ event: "decision-required", serial: "S1", detail: "com.example.maps" });
    assert.equal(normalizeProgressPayload(decodeProgressPayload(raw)), "decision-required");
  });

  it("falls back to a bare event name string", () => {
    assert.equal(normalizeProgressPayload(decodeProgressPayload("completed")), "completed");
  });

  it("maps garbage to null without throwing", () => {
    assert.equal(normalizeProgressPayload(decodeProgressPayload("{{{not json")), null);
    assert.equal(normalizeProgressPayload(decodeProgressPayload('{"event":"bogus"}')), null);
    assert.equal(normalizeProgressPayload(decodeProgressPayload(null)), null);
  });

  it("keeps object payloads working (already-structured events)", () => {
    assert.equal(
      normalizeProgressPayload(decodeProgressPayload({ event: "started", serial: null, detail: null })),
      "started",
    );
  });

  it("listenProgress handler payload type is honest (string|object, F1)", () => {
    // Type-level pin: the handler must accept a bare event-name string, so
    // no caller can trust `.event`/`.serial` without narrowing. If the
    // signature regresses to ProgressPayload-only, svelte-check fails on
    // the `bare` assignment below.
    type Payload = Parameters<Parameters<typeof listenProgress>[0]>[0];
    const bare: Payload = "completed";
    const structured: Payload = { event: "started", serial: null, detail: null };
    assert.equal(bare, "completed");
    assert.equal((structured as { event: string }).event, "started");
  });
});

describe("FIX 2: icon loader caches, dedups in-flight, and falls back on miss", () => {
  it("fires once per packageId across concurrent requests (in-flight dedup)", async () => {
    let calls = 0;
    const loader = createIconLoader(async (packageId: string) => {
      calls += 1;
      await new Promise((r) => setTimeout(r, 10));
      return `data:image/png;base64,${packageId}`;
    });
    const [first, second] = await Promise.all([
      loader.get("com.example.maps"),
      loader.get("com.example.maps"),
    ]);
    assert.equal(calls, 1);
    assert.equal(first, second);
  });

  it("caches successes so a second get never refires", async () => {
    let calls = 0;
    const loader = createIconLoader(async () => {
      calls += 1;
      return "data:image/png;base64,AAAA";
    });
    assert.equal(await loader.get("com.example.maps"), "data:image/png;base64,AAAA");
    assert.equal(await loader.get("com.example.maps"), "data:image/png;base64,AAAA");
    assert.equal(calls, 1);
  });

  it("falls back to null on miss, empty, garbage, and loader errors", async () => {
    assert.equal(resolveIconSrc(null), null);
    assert.equal(resolveIconSrc(""), null);
    assert.equal(resolveIconSrc('{"missing":true}'), null);
    assert.equal(resolveIconSrc("not-a-data-url"), null);
    assert.equal(resolveIconSrc("data:image/png;base64,AAAA"), "data:image/png;base64,AAAA");

    const failing = createIconLoader(async () => {
      throw new Error("gone");
    });
    assert.equal(await failing.get("com.example.gone"), null);
    const missing = createIconLoader(async () => '{"missing":true}');
    assert.equal(await missing.get("com.example.gone"), null);
  });

  it("rejects foreign schemes and malformed base64 (F2)", () => {
    assert.equal(resolveIconSrc("javascript:alert(1)"), null);
    assert.equal(resolveIconSrc("data:text/html,<svg></svg>"), null);
    assert.equal(resolveIconSrc("data:image/svg+xml;base64,AAAA"), null);
    assert.equal(resolveIconSrc("data:image/png;base64,!!!"), null);
    assert.equal(resolveIconSrc("data:image/png;base64,AAA"), null);
    assert.equal(resolveIconSrc("data:image/png;base64,AA=A"), null);
    assert.equal(resolveIconSrc("data:image/png;base64,AAAA"), "data:image/png;base64,AAAA");
    assert.equal(resolveIconSrc("data:image/png;base64,AAA="), "data:image/png;base64,AAA=");
    // Realistic 8-byte PNG header encoding (ends with single padding).
    assert.equal(resolveIconSrc("data:image/png;base64,iVBORw0KGgo="), "data:image/png;base64,iVBORw0KGgo=");
  });
});

describe("FIX 2b: row resolution prefers the fresh catalog over stale cache (F3)", () => {
  it("prefers iconCached===false over any stale cache hit", () => {
    const entry = flagged({ packageId: "com.example.maps", label: "Maps", iconCached: false });
    assert.deepEqual(selectRowIcon(entry, "data:image/png;base64,AAAA"), { kind: "value", src: null });
    assert.deepEqual(selectRowIcon(entry, null), { kind: "value", src: null });
    assert.deepEqual(selectRowIcon(entry, undefined), { kind: "value", src: null });
  });

  it("uses the cache only while the catalog still says cached", () => {
    const entry = flagged({ packageId: "com.example.maps", label: "Maps", iconCached: true });
    assert.deepEqual(selectRowIcon(entry, "data:image/png;base64,AAAA"), {
      kind: "value",
      src: "data:image/png;base64,AAAA",
    });
    assert.deepEqual(selectRowIcon(entry, null), { kind: "value", src: null });
    assert.deepEqual(selectRowIcon(entry, undefined), { kind: "load" });
  });

  it("App drops the icon cache on every new inspection (static, F3)", () => {
    const here = dirname(fileURLToPath(import.meta.url));
    const source = readFileSync(join(here, "..", "App.svelte"), "utf8");
    const body = source.slice(source.indexOf("function handleInspected"), source.indexOf("function handleSnapshot"));
    assert.ok(body.includes("makeIconLoader"), "handleInspected must recreate the icon loader");
  });
});

describe("FIX 3: catalog store flags drive review grouping", () => {
  it("parses isStore/isInstallSource from inspect entries", () => {
    const parsed = parseInspectResult({
      serial: "S1",
      model: "Pixel",
      manufacturer: "Google",
      api: 34,
      entries: [
        { packageId: "com.android.vending", label: "Play Store", isStore: true, isInstallSource: true },
        { packageId: "com.example.maps", label: "Maps", isStore: false, isInstallSource: false },
      ],
    });
    assert.equal(parsed.entries[0]?.isStore, true);
    assert.equal(parsed.entries[0]?.isInstallSource, true);
    assert.equal(parsed.entries[1]?.isStore, false);
  });

  it("groups deselected flagged entries as stores even when the name is not store-like", () => {
    const entries = [
      flagged({ packageId: "com.vendor.updater", label: "System Updater", isStore: true }),
      flagged({ packageId: "com.example.maps", label: "Maps" }),
    ];
    const groups = reviewGroups(entries, {
      "com.vendor.updater": false,
      "com.example.maps": false,
    });
    assert.deepEqual(groups.stores.map((e) => e.packageId), ["com.vendor.updater"]);
    assert.deepEqual(groups.blocked.map((e) => e.packageId), ["com.example.maps"]);
  });

  it("keeps legacy entries without flags on the whole-token heuristic", () => {
    const legacy = [
      { packageId: "com.android.vending", label: "Play Store" },
      { packageId: "org.videolan.vlc", label: "VLC Player" },
    ] as unknown as AppEntryDto[];
    const groups = reviewGroups(legacy, {
      "com.android.vending": false,
      "org.videolan.vlc": false,
    });
    assert.deepEqual(groups.stores.map((e) => e.packageId), ["com.android.vending"]);
    assert.deepEqual(groups.blocked.map((e) => e.packageId), ["org.videolan.vlc"]);
  });

  it("keeps selected flagged entries in kept, never in stores", () => {
    const entries = [flagged({ packageId: "com.android.vending", label: "Play Store", isStore: true })];
    const groups = reviewGroups(entries, { "com.android.vending": true });
    assert.deepEqual(groups.kept.map((e) => e.packageId), ["com.android.vending"]);
    assert.deepEqual(groups.stores, []);
  });

  it("flagged-false wins over store-like names (F5)", () => {
    const entries = [
      flagged({ packageId: "com.android.vending", label: "Play Store", isStore: false, isInstallSource: false }),
    ];
    const groups = reviewGroups(entries, { "com.android.vending": false });
    assert.deepEqual(groups.blocked.map((e) => e.packageId), ["com.android.vending"]);
    assert.deepEqual(groups.stores, []);
    assert.equal(pillForEntry(entries[0]!, false), "blocked");
  });
});
