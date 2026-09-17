/**
 * Task 17 RED: pure selection/filter/review logic in workspace.ts.
 *
 * Runs on the Node built-in test runner (no added dependencies):
 *   node --test src/tests/choose-review.test.ts
 *
 * Covers the closed acceptance checklist items 1-4 plus the large-catalog
 * and no-device-invalidation cases from item 10:
 * - allowlist-first SelectionModel (init/toggle/lock/counts/reset/packageId keys)
 * - filterApps (case-insensitive label+packageId search, All/Kept/Blocked, purity)
 * - reviewGroups (kept/blocked/stores/protected/unsupported/baselineLauncher)
 * - canApply / countText / applyBlockReason / fallbackInitial
 */
import { describe, it } from "node:test";
import assert from "node:assert/strict";

import {
  BASELINE_LAUNCHER_REASON,
  CONNECTION_COPY,
  STORE_TOKENS,
  UNSUPPORTED_REASON,
  ConnectionFlow,
  applyBlockReason,
  canApply,
  countText,
  createSelection,
  fallbackInitial,
  filterApps,
  isStoreLike,
  pillForEntry,
  reviewGroups,
  selectionCounts,
  selectionKeyFor,
  storeTokensFor,
  toggleSelection,
  type SelectionMap,
  type WorkspaceSnapshot,
} from "../lib/state/workspace.ts";
import type { AppEntryDto } from "../lib/api/types.ts";

function entry(overrides: Partial<AppEntryDto> & { packageId: string; label: string }): AppEntryDto {
  return {
    suspended: false,
    enabled: true,
    protected: false,
    protectedReason: null,
    iconCached: false,
    ...overrides,
  };
}

describe("createSelection is allowlist-first and keyed by packageId", () => {
  it("defaults protected entries to kept and everything else to blocked", () => {
    const entries = [
      entry({ packageId: "com.example.maps", label: "Maps" }),
      entry({ packageId: "com.android.systemui", label: "System UI", protected: true, protectedReason: "System UI" }),
    ];
    const selection = createSelection(entries);
    assert.equal(selection["com.example.maps"], false);
    assert.equal(selection["com.android.systemui"], true);
  });

  it("keeps duplicate labels distinct via packageId keys", () => {
    const entries = [
      entry({ packageId: "com.vendor.one", label: "Gallery" }),
      entry({ packageId: "com.vendor.two", label: "Gallery" }),
    ];
    const toggled = toggleSelection(createSelection(entries), entries[0]!);
    assert.equal(toggled["com.vendor.one"], true);
    assert.equal(toggled["com.vendor.two"], false);
  });

  it("resets on a new inspection: stale packages dropped, new packages get defaults", () => {
    const first = [
      entry({ packageId: "com.example.keep", label: "Keep" }),
      entry({ packageId: "com.example.gone", label: "Gone" }),
    ];
    let selection = createSelection(first);
    selection = toggleSelection(selection, first[0]!);
    assert.equal(selection["com.example.keep"], true);

    const second = [
      entry({ packageId: "com.example.keep", label: "Keep" }),
      entry({ packageId: "com.example.new", label: "New" }),
      entry({ packageId: "com.android.settings", label: "Settings", protected: true, protectedReason: "Settings" }),
    ];
    const reset = createSelection(second);
    assert.equal("com.example.gone" in reset, false);
    // Fresh defaults, not carried-over toggles: previously kept package resets to blocked.
    assert.equal(reset["com.example.keep"], false);
    assert.equal(reset["com.example.new"], false);
    assert.equal(reset["com.android.settings"], true);
  });

  it("an empty inspection yields an empty selection (no-device invalidation)", () => {
    assert.deepEqual(createSelection([]), {});
  });

  it("selectionKeyFor keeps back-nav stable and resets on a new inspection", () => {
    const first = [
      entry({ packageId: "com.example.a", label: "A" }),
      entry({ packageId: "com.example.b", label: "B" }),
    ];
    const sameOrder = [
      entry({ packageId: "com.example.a", label: "A renamed" }),
      entry({ packageId: "com.example.b", label: "B" }),
    ];
    const changed = [
      entry({ packageId: "com.example.a", label: "A" }),
      entry({ packageId: "com.example.c", label: "C" }),
    ];
    // Same package set (Choose <-> Review round-trip): key stable, toggles retained.
    assert.equal(selectionKeyFor(sameOrder), selectionKeyFor(first));
    // Key is order-insensitive: a catalog shift alone never reseeds.
    assert.equal(selectionKeyFor([first[1]!, first[0]!]), selectionKeyFor(first));
    // New inspection: key changes, so the host re-seeds defaults.
    assert.ok(selectionKeyFor(changed) !== selectionKeyFor(first));
    assert.equal(selectionKeyFor([]), "");
  });
});

describe("toggleSelection flips only non-protected entries", () => {
  it("flips blocked to kept and back", () => {
    const target = entry({ packageId: "com.example.maps", label: "Maps" });
    const kept = toggleSelection(createSelection([target]), target);
    assert.equal(kept["com.example.maps"], true);
    assert.equal(toggleSelection(kept, target)["com.example.maps"], false);
  });

  it("is a no-op for protected entries", () => {
    const target = entry({
      packageId: "com.android.systemui",
      label: "System UI",
      protected: true,
      protectedReason: "System UI",
    });
    const before = createSelection([target]);
    const after = toggleSelection(before, target);
    assert.deepEqual(after, before);
    assert.equal(after["com.android.systemui"], true);
  });

  it("never mutates the input selection object", () => {
    const target = entry({ packageId: "com.example.maps", label: "Maps" });
    const before = createSelection([target]);
    const snapshot: SelectionMap = { ...before };
    toggleSelection(before, target);
    assert.deepEqual(before, snapshot);
  });
});

describe("selectionCounts", () => {
  it("counts kept (incl. protected), blocked, and total", () => {
    const entries = [
      entry({ packageId: "a", label: "A" }),
      entry({ packageId: "b", label: "B" }),
      entry({ packageId: "p", label: "P", protected: true, protectedReason: "Settings" }),
    ];
    let selection = createSelection(entries);
    assert.deepEqual(selectionCounts(selection, entries), { kept: 1, blocked: 2, total: 3 });
    selection = toggleSelection(selection, entries[0]!);
    assert.deepEqual(selectionCounts(selection, entries), { kept: 2, blocked: 1, total: 3 });
  });

  it("is zeroed for an empty catalog", () => {
    assert.deepEqual(selectionCounts({}, []), { kept: 0, blocked: 0, total: 0 });
  });
});

describe("filterApps searches label and packageId without mutating selection", () => {
  const entries = [
    entry({ packageId: "com.example.maps", label: "Maps" }),
    entry({ packageId: "com.example.gallery", label: "Gallery" }),
    entry({ packageId: "org.other.player", label: "Music" }),
  ];
  const selection: SelectionMap = { "com.example.maps": true, "com.example.gallery": false, "org.other.player": false };

  it("matches labels case-insensitively", () => {
    assert.deepEqual(
      filterApps(entries, selection, "maps", "all").map((e) => e.packageId),
      ["com.example.maps"],
    );
    assert.deepEqual(
      filterApps(entries, selection, "GALLERY", "all").map((e) => e.packageId),
      ["com.example.gallery"],
    );
  });

  it("matches packageIds even when the label differs", () => {
    assert.deepEqual(
      filterApps(entries, selection, "org.other", "all").map((e) => e.packageId),
      ["org.other.player"],
    );
    assert.deepEqual(
      filterApps(entries, selection, "COM.EXAMPLE", "all").map((e) => e.packageId),
      ["com.example.maps", "com.example.gallery"],
    );
  });

  it("filters All, Kept (incl. protected), and Blocked", () => {
    const withProtected = [
      ...entries,
      entry({ packageId: "sys", label: "System UI", protected: true, protectedReason: "System UI" }),
    ];
    const withSel: SelectionMap = { ...selection, sys: true };
    assert.equal(filterApps(withProtected, withSel, "", "all").length, 4);
    assert.deepEqual(
      filterApps(withProtected, withSel, "", "kept").map((e) => e.packageId).sort(),
      ["com.example.maps", "sys"],
    );
    assert.deepEqual(
      filterApps(withProtected, withSel, "", "blocked").map((e) => e.packageId).sort(),
      ["com.example.gallery", "org.other.player"],
    );
  });

  it("trims surrounding whitespace before matching", () => {
    assert.deepEqual(
      filterApps(entries, selection, "maps ", "all").map((e) => e.packageId),
      ["com.example.maps"],
    );
    assert.deepEqual(
      filterApps(entries, selection, "  GALLERY  ", "all").map((e) => e.packageId),
      ["com.example.gallery"],
    );
    assert.deepEqual(filterApps(entries, selection, "   ", "all").length, 3);
  });

  it("combines search text with the active filter", () => {
    assert.deepEqual(
      filterApps(entries, selection, "example", "blocked").map((e) => e.packageId),
      ["com.example.gallery"],
    );
  });

  it("never mutates the selection object", () => {
    const frozen = Object.freeze({ ...selection });
    const found = filterApps(entries, frozen, "maps", "kept");
    assert.equal(found.length, 1);
    assert.deepEqual({ ...frozen }, selection);
  });
});

describe("reviewGroups partitions the catalog", () => {
  const entries = [
    entry({ packageId: "com.example.maps", label: "Maps" }),
    entry({ packageId: "com.android.vending", label: "Play Store" }),
    entry({ packageId: "com.example.sideload", label: "Sideload Helper" }),
    entry({ packageId: "com.android.systemui", label: "System UI", protected: true, protectedReason: "System UI" }),
    entry({
      packageId: "com.vendor.blur",
      label: "Blur Service",
      protected: true,
      protectedReason: "manufacturer or shared-role dependency",
    }),
    entry({
      packageId: "com.vendor.mystery",
      label: "Mystery",
      protected: true,
      protectedReason: "Kept for safety: role state is unresolved or ambiguous",
    }),
    entry({
      packageId: "com.example.home",
      label: "Old Home",
      protected: true,
      protectedReason: "baseline launcher",
    }),
  ];
  const selection: SelectionMap = {
    "com.example.maps": true,
    "com.android.vending": false,
    "com.example.sideload": false,
    "com.android.systemui": true,
    "com.vendor.blur": true,
    "com.vendor.mystery": true,
    "com.example.home": true,
  };

  it("keeps selected non-protected apps and blocks the rest minus stores", () => {
    const groups = reviewGroups(entries, selection);
    assert.deepEqual(groups.kept.map((e) => e.packageId), ["com.example.maps"]);
    assert.deepEqual(groups.blocked.map((e) => e.packageId), []);
  });

  it("groups deselected store-like apps via whole-token matching", () => {
    assert.ok(STORE_TOKENS.has("play") && STORE_TOKENS.has("vending") && STORE_TOKENS.has("installer"));
    const groups = reviewGroups(entries, selection);
    assert.deepEqual(
      groups.stores.map((e) => e.packageId).sort(),
      ["com.android.vending", "com.example.sideload"],
    );
    assert.ok(isStoreLike(entries[1]!));
    assert.ok(!isStoreLike(entries[0]!));
  });

  it("keeps protected entries out of kept/blocked and splits off unsupported", () => {
    const groups = reviewGroups(entries, selection);
    assert.deepEqual(groups.protected.map((e) => e.packageId).sort(), ["com.android.systemui", "com.example.home"]);
    assert.deepEqual(
      groups.unsupported.map((e) => e.packageId).sort(),
      ["com.vendor.blur", "com.vendor.mystery"],
    );
    assert.match("manufacturer or shared-role dependency", UNSUPPORTED_REASON);
    assert.match("unresolved or ambiguous", UNSUPPORTED_REASON);
  });

  it("flags the baseline launcher separately while it stays in the protected group", () => {
    assert.match("baseline launcher", BASELINE_LAUNCHER_REASON);
    const groups = reviewGroups(entries, selection);
    assert.equal(groups.baselineLauncher?.packageId, "com.example.home");
    assert.ok(groups.protected.some((e) => e.packageId === "com.example.home"));
  });

  it("leaves a selected store-like app in kept (stores come only from deselected apps)", () => {
    const sel: SelectionMap = { ...selection, "com.android.vending": true };
    const groups = reviewGroups(entries, sel);
    assert.ok(groups.kept.some((e) => e.packageId === "com.android.vending"));
    assert.ok(!groups.stores.some((e) => e.packageId === "com.android.vending"));
  });

  it("counts reconcile across every group", () => {
    const groups = reviewGroups(entries, selection);
    const total = groups.kept.length + groups.blocked.length + groups.stores.length + groups.protected.length + groups.unsupported.length;
    assert.equal(total, entries.length);
  });

  it("handles an empty catalog", () => {
    const groups = reviewGroups([], {});
    assert.deepEqual(groups.kept, []);
    assert.deepEqual(groups.blocked, []);
    assert.deepEqual(groups.stores, []);
    assert.deepEqual(groups.protected, []);
    assert.deepEqual(groups.unsupported, []);
    assert.equal(groups.baselineLauncher, null);
  });
});

describe("isStoreLike matches whole tokens, never substrings (F1)", () => {
  const cases: Array<[string, string, boolean]> = [
    // Ordinary apps whose names merely contain store-ish substrings.
    ["org.videolan.vlc", "VLC Player", false],
    ["com.example.displaytester", "Display Tester", false],
    ["com.example.supermarket", "SuperMarket List", false],
    ["com.example.installments", "Installment Tracker", false],
    // Genuine stores and install sources.
    ["com.android.vending", "Play Store", true],
    ["com.example.sideload", "Sideload Helper", true],
    ["com.android.packageinstaller", "Package Installer", true],
    ["com.example.market", "Bazaar Market", true],
  ];

  for (const [packageId, label, expected] of cases) {
    it(`${label} (${packageId}) -> ${expected ? "store" : "not a store"}`, () => {
      assert.equal(isStoreLike(entry({ packageId, label })), expected);
    });
  }

  it("tokenizes packageId and label on non-alphanumeric boundaries", () => {
    assert.deepEqual(storeTokensFor(entry({ packageId: "com.android.vending", label: "Play Store" })), [
      "com",
      "android",
      "vending",
      "play",
      "store",
    ]);
  });

  it("groups ordinary substring-matching apps as blocked, never stores", () => {
    const ordinary = [
      entry({ packageId: "org.videolan.vlc", label: "VLC Player" }),
      entry({ packageId: "com.example.displaytester", label: "Display Tester" }),
      entry({ packageId: "com.example.supermarket", label: "SuperMarket List" }),
      entry({ packageId: "com.example.installments", label: "Installment Tracker" }),
    ];
    const groups = reviewGroups(ordinary, createSelection(ordinary));
    assert.equal(groups.stores.length, 0);
    assert.equal(groups.blocked.length, 4);
  });
});

describe("pillForEntry labels chooser rows exactly like review groups (F4)", () => {
  it("marks unsupported protected entries Unsupported", () => {
    const target = entry({
      packageId: "com.vendor.blur",
      label: "Blur Service",
      protected: true,
      protectedReason: "manufacturer or shared-role dependency",
    });
    assert.equal(pillForEntry(target, true), "unsupported");
  });

  it("keeps the baseline launcher and ordinary protected entries Protected", () => {
    assert.equal(
      pillForEntry(
        entry({ packageId: "h", label: "Old Home", protected: true, protectedReason: "baseline launcher" }),
        true,
      ),
      "protected",
    );
    assert.equal(
      pillForEntry(
        entry({ packageId: "s", label: "System UI", protected: true, protectedReason: "System UI" }),
        true,
      ),
      "protected",
    );
  });

  it("labels non-protected entries kept, store, or blocked", () => {
    assert.equal(pillForEntry(entry({ packageId: "a", label: "Maps" }), true), "kept");
    assert.equal(pillForEntry(entry({ packageId: "v", label: "Play Store" }), false), "store");
    assert.equal(pillForEntry(entry({ packageId: "m", label: "Maps" }), false), "blocked");
  });
});

describe("canApply gates Apply on ready + new-setup + non-empty catalog", () => {
  const entries = [entry({ packageId: "a", label: "A" })];
  const session = { kind: "new-setup" as const, guidance: "", allowedActions: ["begin-setup" as const] };

  it("is true only when every gate passes", () => {
    assert.equal(canApply({ connection: "ready", session, entries }), true);
  });

  it("is false for any failed gate", () => {
    assert.equal(canApply({ connection: "idle", session, entries }), false);
    assert.equal(canApply({ connection: "ready", session: null, entries }), false);
    assert.equal(
      canApply({ connection: "ready", session: { ...session, kind: "active-policy" }, entries }),
      false,
    );
    assert.equal(canApply({ connection: "ready", session, entries: [] }), false);
    assert.equal(canApply({ connection: "recovery-required", session, entries }), false);
  });
});

describe("applyBlockReason explains a disabled Apply in text", () => {
  const entries = [entry({ packageId: "a", label: "A" })];
  const session = { kind: "new-setup" as const, guidance: "", allowedActions: ["begin-setup" as const] };

  it("is null when Apply is allowed", () => {
    assert.equal(applyBlockReason("ready", session, entries), null);
  });

  it("names the failing gate in plain words", () => {
    assert.match(applyBlockReason("idle", session, entries) ?? "", /connect|inspect/i);
    assert.match(applyBlockReason("ready", null, entries) ?? "", /setup|session|inspect/i);
    assert.match(
      applyBlockReason("ready", { ...session, kind: "active-policy" }, entries) ?? "",
      /already|reconcile/i,
    );
    assert.match(applyBlockReason("ready", session, []) ?? "", /no apps|nothing/i);
  });
});

describe("countText renders counts as text, not color alone", () => {
  it("formats kept, blocked, and total", () => {
    assert.equal(countText(3, 12, 15), "3 kept · 12 blocked · 15 total");
    assert.equal(countText(0, 0, 0), "0 kept · 0 blocked · 0 total");
  });
});

describe("fallbackInitial documents the neutral icon fallback", () => {
  it("uses the first letter of the label", () => {
    assert.equal(fallbackInitial("Maps"), "M");
    assert.equal(fallbackInitial(" gallery"), "G");
  });

  it("falls back to a neutral mark for empty labels", () => {
    assert.equal(fallbackInitial(""), "?");
    assert.equal(fallbackInitial("   "), "?");
  });
});

describe("ConnectionFlow.restoreSnapshot preserves an inspection across remounts (F5)", () => {
  const savedEntry = entry({ packageId: "com.example.maps", label: "Maps" });

  function stubDeps() {
    return {
      discoverDevices: async () => ({ ok: true as const, value: { serial: "S1" } }),
      inspectDevice: async (_serial: string) => ({ ok: true as const, value: [] }),
      getSession: async (_s: string, _f: string) => ({
        ok: true as const,
        value: { kind: "new-setup" as const, guidance: "", allowedActions: ["begin-setup" as const] },
      }),
    };
  }

  function savedSnapshot(): WorkspaceSnapshot {
    return {
      connection: "ready",
      guidance: {
        state: "ready" as const,
        tone: "info" as const,
        heading: CONNECTION_COPY.ready.heading,
        body: CONNECTION_COPY.ready.body,
        action: CONNECTION_COPY.ready.action,
      },
      device: null,
      entries: [savedEntry],
      session: { kind: "new-setup" as const, guidance: "", allowedActions: ["begin-setup" as const] },
      busy: true,
    };
  }

  it("restores entries/session/connection without going busy", () => {
    const announced: string[] = [];
    const flow = new ConnectionFlow(stubDeps(), { announce: (m) => void announced.push(m) });
    flow.restoreSnapshot(savedSnapshot());
    assert.equal(flow.snapshot.connection, "ready");
    assert.deepEqual(flow.snapshot.entries, [savedEntry]);
    assert.equal(flow.snapshot.session?.kind, "new-setup");
    assert.equal(flow.snapshot.busy, false);
    assert.equal(announced.length, 1);
  });
});

describe("large catalogs stay fast and reconcile", () => {
  it("filters and groups 3000 entries with reconciled counts", () => {
    const big: AppEntryDto[] = [];
    for (let i = 0; i < 3000; i += 1) {
      if (i % 300 === 1) {
        big.push(entry({ packageId: `com.example.store${i}`, label: `Store ${i}` }));
      } else if (i % 50 === 0) {
        const unsupported = i % 200 === 0;
        big.push(
          entry({
            packageId: `com.sys.comp${i}`,
            label: `System component ${i}`,
            protected: true,
            protectedReason: unsupported ? "manufacturer or shared-role dependency" : "System UI",
          }),
        );
      } else {
        big.push(entry({ packageId: `com.example.app${i}`, label: `App ${i}` }));
      }
    }
    let selection = createSelection(big);
    selection = toggleSelection(selection, big[2]!);
    selection = toggleSelection(selection, big[3]!);

    const filtered = filterApps(big, selection, "app1", "all");
    assert.ok(filtered.length > 0);
    assert.ok(filtered.every((e) => e.label.toLowerCase().includes("app1") || e.packageId.toLowerCase().includes("app1")));

    const groups = reviewGroups(big, selection);
    const total =
      groups.kept.length + groups.blocked.length + groups.stores.length + groups.protected.length + groups.unsupported.length;
    assert.equal(total, 3000);
    assert.equal(groups.kept.length, 2);
    assert.equal(groups.stores.length, 10);
    const counts = selectionCounts(selection, big);
    assert.equal(counts.total, 3000);
    assert.equal(counts.kept, groups.kept.length + groups.protected.length + groups.unsupported.length);
    assert.equal(countText(counts.kept, counts.blocked, counts.total), `${counts.kept} kept · ${counts.blocked} blocked · 3000 total`);
  });
});
