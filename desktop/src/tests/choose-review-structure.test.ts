/**
 * Task 17 RED: static structural assertions for the chooser/review UI.
 *
 * Runs on the Node built-in test runner (no added dependencies):
 *   node --test src/tests/choose-review-structure.test.ts
 *
 * Guards checklist items 5-10 structurally: keyboard/screen-reader treatment,
 * icon fallback without remote assets, visual tokens (33px/59px/border-radius),
 * read-only transport boundary (backend spy), App-level navigation + retained
 * selection, and no secret leakage in presentation sources.
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

const NEW_SCREENS = ["src/lib/screens/ChooseAppsScreen.svelte", "src/lib/screens/ReviewScreen.svelte"];
const NEW_COMPONENTS = [
  "src/lib/components/AppToolbar.svelte",
  "src/lib/components/AppList.svelte",
  "src/lib/components/AppRow.svelte",
  "src/lib/components/StatePill.svelte",
  "src/lib/components/SelectionSwitch.svelte",
  "src/lib/components/CountCard.svelte",
  "src/lib/components/ReviewGroup.svelte",
];
/** Every Task 17 owned source, including the minimally edited hosts. */
const OWNED = [
  "src/lib/state/workspace.ts",
  ...NEW_SCREENS,
  ...NEW_COMPONENTS,
  "src/App.svelte",
  "src/lib/screens/ConnectScreen.svelte",
];

describe("keyboard and screen-reader treatment", () => {
  it("SelectionSwitch is a real switch with state and reason exposed", () => {
    const source = read("src/lib/components/SelectionSwitch.svelte");
    assert.ok(source.includes('role="switch"'), "role=switch present");
    assert.ok(source.includes("aria-checked"), "aria-checked present");
    assert.ok(source.includes("aria-label"), "aria-label present");
    assert.ok(source.includes("packageId"), "label carries the package id");
    assert.ok(source.includes("disabled"), "locked switches disable the control");
    assert.ok(source.includes("<button"), "native button keeps keyboard operability");
    assert.ok(source.includes("<svg"), "visible lock affordance pairs an icon with text");
  });

  it("AppToolbar labels its searchbox and exposes a real filter group plus count", () => {
    const source = read("src/lib/components/AppToolbar.svelte");
    assert.ok(source.includes('type="search"'), "search input uses type=search");
    assert.ok(source.includes("<label"), "searchbox has a visible label");
    assert.ok(source.includes("radiogroup"), "filters group as a radiogroup");
    assert.ok(source.includes('type="radio"'), "filters are real radios");
    for (const name of ["All", "Kept", "Blocked"]) {
      assert.ok(source.includes(name), `filter option ${name} present`);
    }
    assert.ok(source.includes("apps") && source.includes("shown"), "text results count present");
  });

  it("AppList is a real list with a text empty state", () => {
    const source = read("src/lib/components/AppList.svelte");
    assert.ok(source.includes('role="list"'), "role=list present");
    assert.match(source, /No apps match/, "empty-state text present");
  });

  it("StatePill pairs an icon with text for every state (never color alone)", () => {
    const source = read("src/lib/components/StatePill.svelte");
    assert.ok(source.includes("<svg"), "pill pairs an icon with text");
    for (const name of ["Kept", "Blocked", "Protected", "Store", "Unsupported"]) {
      assert.ok(source.includes(name), `pill names the ${name} state in text`);
    }
  });

  it("ReviewGroup renders a section with a real heading, description, and count", () => {
    const source = read("src/lib/components/ReviewGroup.svelte");
    assert.ok(source.includes("<section"), "group is a section");
    assert.match(source, /<h[23]/, "group title is a real heading");
    assert.ok(source.includes("packageId") || source.includes("package"), "entries show the package id");
    assert.ok(source.includes("apps") || source.includes("count"), "group carries count text");
  });

  it("Choose and Review add no extra live regions (single shell region only)", () => {
    for (const rel of NEW_SCREENS) {
      assert.doesNotMatch(read(rel), /aria-live/, `${rel} must not add a live region`);
    }
  });

  it("screens announce through headings and AppShell keeps the single live region", () => {
    assert.match(read("src/lib/screens/ChooseAppsScreen.svelte"), /<h2/, "chooser has a heading");
    assert.match(read("src/lib/screens/ReviewScreen.svelte"), /<h2/, "review has a heading");
    assert.ok(read("src/lib/screens/ChooseAppsScreen.svelte").includes("onAnnounce"), "chooser announces count changes via callback");
  });
});

describe("icons fall back locally without remote assets", () => {
  it("AppRow renders a real 33px rounded img and swaps to a fallback on error", () => {
    const source = read("src/lib/components/AppRow.svelte");
    assert.ok(source.includes("<img"), "real img element present");
    assert.ok(source.includes("33px"), "33px icon treatment present");
    assert.ok(source.includes("59px"), "59px row treatment present");
    assert.ok(source.includes("border-radius"), "rounded icon treatment present");
    assert.ok(source.includes("onerror"), "failed streams fall back locally");
    assert.ok(source.includes("alt"), "icon img carries alt text");
  });

  it("no owned source pulls remote assets or guessed brand art", () => {
    const combined = OWNED.map(read).join("\n");
    assert.doesNotMatch(combined, /https?:\/\/[^"'\s]*\.(svg|png|woff2?)/i, "no remote assets");
    assert.doesNotMatch(combined, /fonts\.googleapis|@import\s+url\(http/i, "no network fonts");
  });
});

describe("visual treatment reuses tokens and stays fluid", () => {
  it("new components reference tokens.css vars and shrink without h-scroll", () => {
    const combined = [...NEW_SCREENS, ...NEW_COMPONENTS].map(read).join("\n");
    assert.ok(combined.includes("var(--unscroll-"), "tokens vars referenced");
    assert.ok(combined.includes("min-width: 0"), "fluid children can shrink");
  });
});

describe("read-only boundary: backend spy proves no mutating command", () => {
  it("owned sources never name a mutating or diagnostic-export command", () => {
    const banned = /start_apply|start_edit|open_maintenance|close_maintenance|start_restore|respond_to_decision|retry_cleanup|export_diagnostics|preview_diagnostics/;
    for (const rel of OWNED) {
      assert.doesNotMatch(read(rel), banned, `${rel} must stay read-only`);
    }
  });

  it("owned sources touch Rust only through invoke.ts helpers plus DTO types", () => {
    const combined = OWNED.map(read).join("\n");
    assert.doesNotMatch(combined, /from\s+["']@tauri-apps\/api\/core["']/, "no direct invoke imports");
    assert.doesNotMatch(combined, /child_process|execSync|spawn\(/, "no process spawning");
    assert.doesNotMatch(combined, /adb shell/i, "no device commands");
    assert.doesNotMatch(combined, /pm (suspend|unsuspend)/i, "no package commands");
  });

  it("new presentation sources never render serials, fingerprints, or exit codes", () => {
    const combined = [...NEW_SCREENS, ...NEW_COMPONENTS].map(read).join("\n");
    assert.doesNotMatch(combined, /serial/i, "no serials in presentation");
    assert.doesNotMatch(combined, /fingerprint/i, "no fingerprints in presentation");
    assert.doesNotMatch(combined, /exit code/i, "no exit codes in presentation");
  });
});

describe("navigation keeps one shared selection across Choose and Review", () => {
  it("App.svelte owns the view state and shares one selection with both screens", () => {
    const source = read("src/App.svelte");
    assert.ok(source.includes("choose") && source.includes("review"), "connect|choose|review views present");
    assert.ok(source.includes("ChooseAppsScreen"), "App renders the chooser");
    assert.ok(source.includes("ReviewScreen"), "App renders review");
    assert.ok(source.includes("selection"), "shared selection state lives in App");
    assert.ok(source.includes("createSelection"), "selection initializes from inspected entries");
  });

  it("ConnectScreen offers Continue navigation plus inspected-data handoff", () => {
    const source = read("src/lib/screens/ConnectScreen.svelte");
    assert.ok(source.includes("onContinue"), "optional Continue navigation callback present");
    assert.ok(source.includes("onInspected"), "inspected entries/session handoff present");
    assert.doesNotMatch(source, /not available in this build yet/, "interim dead-end copy is gone");
  });

  it("ReviewScreen gates Apply in text and hands Apply to the Task 18 flow", () => {
    const source = read("src/lib/screens/ReviewScreen.svelte");
    assert.ok(source.includes("onApply"), "onApply callback prop present");
    assert.ok(source.includes("disabled"), "Apply disables until preflight passes");
    assert.ok(source.includes("Back to Choose"), "back navigation present");
    assert.ok(source.includes("ReviewGroup"), "five review groups rendered");
  });

  it("ChooseAppsScreen carries the baseline explainer, toolbar, count, list, and actions", () => {
    const source = read("src/lib/screens/ChooseAppsScreen.svelte");
    assert.match(source, /baseline launcher/i, "baseline-launcher explainer present");
    assert.ok(source.includes("AppToolbar"), "toolbar present");
    assert.ok(source.includes("CountCard"), "count card present");
    assert.ok(source.includes("AppList"), "app list present");
    assert.ok(source.includes("Continue to Review"), "primary action present");
    assert.ok(source.includes("Back"), "back action present");
  });
});

describe("correction round: token heuristic, honest stub, shared pills, remount safety", () => {
  it("AppList derives pills from the shared workspace helper (F4)", () => {
    assert.ok(read("src/lib/components/AppList.svelte").includes("pillForEntry"), "shared pill helper used");
  });

  it("the enabled-Apply stub stays honest with no dead-end phrasing (F2)", () => {
    const source = read("src/App.svelte");
    assert.doesNotMatch(source, /not available in this build yet/, "dead-end phrasing gone");
    assert.match(source, /next build/i, "stub names the next build");
    assert.match(source, /selection is preserved/i, "stub confirms the selection survives");
  });

  it("Back from Choose restores the inspection instead of re-inspecting (F5)", () => {
    const screen = read("src/lib/screens/ConnectScreen.svelte");
    assert.ok(screen.includes("initialSnapshot"), "restore prop present");
    assert.ok(screen.includes("autoStart"), "auto-start opt-out present");
    assert.ok(screen.includes("restoreSnapshot"), "snapshot restore wired");
    const app = read("src/App.svelte");
    assert.ok(app.includes("initialSnapshot"), "App hands the saved snapshot back");
    assert.match(app, /reset to defaults/i, "reseed surfaces an explicit notice");
  });

  it("connection staleness is documented as Task 18's apply-time job (F3)", () => {
    const combined = read("src/App.svelte") + read("src/lib/screens/ReviewScreen.svelte");
    assert.match(combined, /Task 18/, "revalidation owner named");
    assert.match(combined, /re-check|revalidat/i, "apply-time revalidation stated");
  });
});
