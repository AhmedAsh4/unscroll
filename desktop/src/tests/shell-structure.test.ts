/**
 * Static semantic/a11y/structure assertions for Task 16 owned files.
 *
 * Runs on the Node built-in test runner (no added dependencies):
 *   node --test src/tests/shell-structure.test.ts
 *
 * These guard the acceptance checklist items that are structural: exact
 * tokens/layout values, semantic landmarks, focus visibility, live regions,
 * reduced motion, system-fonts/local-SVG-only, invoke.ts-only transport use,
 * and no raw command/serial leakage in presentation files.
 */
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const desktop = join(here, "..", "..");
const src = join(desktop, "src");
const repo = join(desktop, "..");

function read(rel: string): string {
  const full = rel.startsWith("docs:") ? join(repo, rel.slice("docs:".length)) : join(desktop, rel);
  assert.ok(existsSync(full), `owned file exists: ${rel}`);
  return readFileSync(full, "utf8");
}

describe("design reference", () => {
  it("tracks the reconstructed shell reference with provenance", () => {
    const html = read("docs:docs/design/unscroll-v1-guided-workspace.html");
    assert.match(html, /provenance/i);
    assert.match(html, /reconstruct/i);
    assert.match(html, /u-shell/);
    assert.match(html, /#f6f3ec/);
    assert.match(html, /#203b31/);
    assert.match(html, /#245c47/);
    assert.match(html, /Connect/);
    assert.match(html, /Choose apps/);
    assert.match(html, /Review/);
    assert.match(html, /Apply/);
  });
});

describe("design tokens", () => {
  it("declares the exact approved palette, type, and layout tokens", () => {
    const css = read("src/styles/tokens.css");
    for (const token of ["#f6f3ec", "#fffdf8", "#203b31", "#245c47", "#183128", "#5f6c65", "#cfd8d1"]) {
      assert.ok(css.includes(token), `tokens.css contains ${token}`);
    }
    assert.match(css, /--unscroll-bar-height:\s*54px/);
    assert.match(css, /--unscroll-rail-width:\s*210px/);
    assert.match(css, /:root/);
  });

  it("keeps content near the 1050px target and collapses without h-scroll", () => {
    const css = read("src/styles/tokens.css") + read("src/styles/global.css") + read("src/lib/components/AppShell.svelte");
    assert.ok(css.includes("1050px"), "1050px content target present");
    assert.ok(css.includes("min-width: 0"), "flex children can shrink (min-width: 0)");
    assert.ok(css.includes("minmax"), "fluid grid/flex track (minmax)");
    assert.ok(css.includes("@media"), "narrow-width rail collapse present");
    assert.ok(css.includes("800px"), "800px minimum-width handling present");
    assert.doesNotMatch(css, /overflow-x:\s*(scroll|auto)/, "no horizontal-scroll escape hatch");
  });

  it("uses system fonts and local SVG only", () => {
    const css = read("src/styles/global.css") + read("src/styles/tokens.css");
    assert.ok(css.includes("system-ui"), "system font stack present");
    assert.doesNotMatch(css, /@import\s+url\(http/i, "no remote font imports");
    assert.doesNotMatch(css, /fonts\.googleapis/i, "no network fonts");
    const svelte = [
      "src/lib/components/AppShell.svelte",
      "src/lib/components/TitleBar.svelte",
      "src/lib/components/StepRail.svelte",
      "src/lib/components/DeviceCard.svelte",
      "src/lib/components/ConnectionStatus.svelte",
      "src/lib/components/InlineNotice.svelte",
      "src/lib/screens/ConnectScreen.svelte",
    ].map(read).join("\n");
    assert.doesNotMatch(svelte, /https?:\/\/[^"'\s]*\.(svg|png|woff2?)/i, "no remote assets");
    assert.ok(svelte.includes("<svg"), "inline SVG icons present");
  });
});

describe("accessibility structure", () => {
  it("keeps semantic landmarks and tab order title bar -> rail -> content -> action", () => {
    const shell = read("src/lib/components/AppShell.svelte");
    assert.ok(shell.includes("<header"), "title bar landmark");
    assert.ok(shell.includes("<nav"), "step rail landmark");
    assert.ok(shell.includes("<main"), "main content landmark");
    const order = [shell.indexOf("<header"), shell.indexOf("<nav"), shell.indexOf("<main")];
    assert.ok(order[0] < order[1] && order[1] < order[2], "landmark order is header, nav, main");
    assert.ok(shell.includes("skip"), "skip link present");
    const rail = read("src/lib/components/StepRail.svelte");
    assert.ok(rail.includes("aria-current"), "current step exposed to screen readers");
    assert.ok(rail.includes("<ol") || rail.includes("<ul"), "steps are a list");
  });

  it("exposes step state without hiding names from screen readers (MAJOR-1)", () => {
    const rail = read("src/lib/components/StepRail.svelte");
    // aria-current lives on the non-hidden li, never on an aria-hidden marker.
    assert.ok(rail.includes("<li"), "steps render list items");
    assert.match(rail, /<li[^>]*aria-current/, "aria-current is on the list item");
    for (const line of rail.split("\n")) {
      assert.ok(
        !(line.includes("aria-current") && line.includes("aria-hidden")),
        "aria-current never shares a hidden ancestor tag",
      );
    }
    // Completed/blocked sr-only status text sits outside hidden markers.
    assert.ok(rail.includes("sr-only"), "status text has an sr-only variant");
    assert.ok(rail.includes("completed") && rail.includes("blocked"), "completed/blocked states named");
    // Collapsed rail keeps names in the a11y tree (no display:none).
    const shell = read("src/lib/components/AppShell.svelte");
    assert.doesNotMatch(shell, /step-label[\s\S]{0,200}?display:\s*none/, "labels not display:none");
    assert.ok(shell.includes("clip: rect"), "collapsed labels use a visually-hidden technique");
  });

  it("announces connection changes through a single polite live region", () => {
    const shell = read("src/lib/components/AppShell.svelte");
    assert.ok(shell.includes('aria-live="polite"'), "polite live region present");
    assert.ok(shell.includes('role="status"'), "status role present");
    const liveCount = (shell.match(/aria-live/g) ?? []).length;
    assert.ok(liveCount <= 2, `live regions stay minimal (found ${liveCount})`);
    const screen = read("src/lib/screens/ConnectScreen.svelte");
    assert.ok(screen.includes("aria-busy"), "busy operation exposes aria-busy");
  });

  it("keeps visible focus and honors reduced motion", () => {
    const css = read("src/styles/global.css");
    assert.ok(css.includes(":focus-visible"), "visible focus indicator present");
    assert.ok(css.includes("prefers-reduced-motion"), "reduced-motion support present");
    const combined = css + read("src/lib/components/AppShell.svelte");
    assert.ok(combined.includes("outline"), "focus uses an outline");
  });

  it("never signals status by color alone", () => {
    const status = read("src/lib/components/ConnectionStatus.svelte");
    assert.ok(status.includes("<svg"), "status pairs an icon with text");
    const notice = read("src/lib/components/InlineNotice.svelte");
    assert.ok(notice.includes("<svg") || notice.includes("icon"), "notices pair an icon with text");
  });

  it("gives notices unique real headings (MINOR-3)", () => {
    const notice = read("src/lib/components/InlineNotice.svelte");
    assert.ok(notice.includes("<h3"), "notice title is a real heading");
    assert.ok(notice.includes("$props.id()"), "heading id is per-instance unique");
    assert.doesNotMatch(notice, /title\.length.*message\.length/, "no length-based colliding id");
  });

  it("omits empty device metadata and keeps passive-wait/honest-continue copy", () => {
    const card = read("src/lib/components/DeviceCard.svelte");
    assert.match(card, /\{#if manufacturer \|\| platformLabel\}/, "empty meta paragraph omitted");
    const screen = read("src/lib/screens/ConnectScreen.svelte");
    assert.ok(screen.includes("Waiting"), "passive-wait adjacent action present");
    assert.ok(screen.includes("actionDisabled"), "wait/continue use visibly disabled state");
    assert.doesNotMatch(screen, /opens in the next step/, "no false navigation promise");
    assert.match(screen, /not available in this build yet/, "honest interim copy present");
  });
});

describe("transport and secrecy invariants", () => {
  it("touches Rust only through invoke.ts helpers plus DTO types", () => {
    const owned = [
      "src/lib/state/workspace.ts",
      "src/lib/screens/ConnectScreen.svelte",
      "src/lib/components/AppShell.svelte",
      "src/lib/components/TitleBar.svelte",
      "src/lib/components/StepRail.svelte",
      "src/lib/components/DeviceCard.svelte",
      "src/lib/components/ConnectionStatus.svelte",
      "src/lib/components/InlineNotice.svelte",
      "src/App.svelte",
    ].map(read).join("\n");
    assert.doesNotMatch(owned, /from\s+["']@tauri-apps\/api\/core["']/, "no direct invoke imports");
    assert.doesNotMatch(owned, /child_process|execSync|spawn\(/, "no process spawning");
    assert.doesNotMatch(owned, /adb shell/i, "no device commands");
    assert.doesNotMatch(owned, /pm (suspend|unsuspend)/i, "no package commands");
  });

  it("never renders raw serials, fingerprints, or exit codes", () => {
    const svelte = [
      "src/lib/components/DeviceCard.svelte",
      "src/lib/components/ConnectionStatus.svelte",
      "src/lib/components/InlineNotice.svelte",
      "src/lib/screens/ConnectScreen.svelte",
      "src/lib/components/TitleBar.svelte",
    ].map(read).join("\n");
    assert.doesNotMatch(svelte, /serial/i, "no serials in presentation");
    assert.doesNotMatch(svelte, /fingerprint/i, "no fingerprints in presentation");
    assert.doesNotMatch(svelte, /exit code/i, "no exit codes in presentation");
  });

  it("wires App.svelte to the shell and connection flow", () => {
    const app = read("src/App.svelte");
    assert.ok(app.includes("AppShell"), "App renders the shell");
    assert.ok(app.includes("ConnectScreen"), "App renders the connect flow");
    assert.ok(app.includes("tokens.css") || app.includes("global.css") || read("src/main.ts").includes(".css"), "styles loaded");
  });
});

void src;
