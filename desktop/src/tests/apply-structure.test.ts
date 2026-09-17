/**
 * Task 18 RED: static structural assertions for apply/decision/HOME/completion UI.
 *
 * Runs on the Node built-in test runner (no added dependencies):
 *   node --test src/tests/apply-structure.test.ts
 *
 * Guards the structural acceptance items: owned files exist, ApplyScreen owns
 * the progress subscription (decode + narrow), re-validates the Review gate,
 * reuses the 300ms persistence marker, announces only via the shell region,
 * moves focus to decision headings, offers only the approved choices, reports
 * partial protection honestly, guides USB debugging safely, gates completion
 * on the backend outcome, and never renders device identity.
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

const NEW_SCREENS = ["src/lib/screens/ApplyScreen.svelte", "src/lib/screens/CompletionScreen.svelte"];
const NEW_COMPONENTS = [
  "src/lib/components/OperationProgress.svelte",
  "src/lib/components/AppFailureDecision.svelte",
  "src/lib/components/LauncherChooserGuide.svelte",
  "src/lib/components/ProtectionSummary.svelte",
  "src/lib/components/UsbDebuggingGuide.svelte",
];
const NEW_ALL = [...NEW_SCREENS, ...NEW_COMPONENTS];

describe("owned files exist", () => {
  for (const rel of NEW_ALL) {
    it(rel, () => {
      assert.ok(existsSync(join(desktop, rel)), `${rel} exists`);
    });
  }
});

describe("ApplyScreen owns progress and the live-device gate", () => {
  it("subscribes to progress and decodes JSON text before narrowing", () => {
    const source = read("src/lib/screens/ApplyScreen.svelte");
    assert.ok(source.includes("listenProgress"), "owns the listenProgress subscription");
    assert.ok(source.includes("decodeProgressPayload"), "decodes JSON-text payloads");
    assert.ok(source.includes("normalizeProgressPayload"), "narrows via the workspace normalizer");
  });

  it("re-validates the Review gate instead of trusting navigation", () => {
    const source = read("src/lib/screens/ApplyScreen.svelte");
    assert.ok(source.includes("canApply"), "Apply screen re-validates the gate");
    const app = read("src/App.svelte");
    assert.ok(app.includes("canApply"), "App navigates to apply only through the gate");
  });

  it("reuses the 300ms persistent-status marker", () => {
    const source = read("src/lib/screens/ApplyScreen.svelte");
    assert.ok(source.includes("SHOW_BUSY_AFTER_MS"), "persistent status reuses the shared marker");
  });

  it("disables Apply while running (no double-start)", () => {
    const source = read("src/lib/screens/ApplyScreen.svelte");
    assert.ok(source.includes("disabled"), "running state disables the control");
    assert.match(source, /busy|running/, "disabled state derives from the running flag");
  });
});

describe("single live region (no nested announcements)", () => {
  it("new components add no live regions or status roles", () => {
    for (const rel of NEW_ALL) {
      assert.doesNotMatch(read(rel), /aria-live/, `${rel} must not add a live region`);
      assert.doesNotMatch(read(rel), /role="status"/, `${rel} must not add a status role`);
      assert.doesNotMatch(read(rel), /role="alert"/, `${rel} must not add an alert role`);
    }
  });

  it("announcements travel through the shell prop callback", () => {
    const source = read("src/lib/screens/ApplyScreen.svelte");
    assert.ok(source.includes("onAnnounce") || source.includes("announcement"), "announces via the shell region");
    assert.ok(read("src/App.svelte").includes("ApplyScreen"), "App wires the apply view into the shell");
  });
});

describe("decision focus and keyboard operability", () => {
  it("moves focus to each decision heading", () => {
    for (const rel of ["src/lib/components/AppFailureDecision.svelte", "src/lib/components/LauncherChooserGuide.svelte"]) {
      const source = read(rel);
      assert.ok(source.includes('tabindex="-1"'), `${rel} heading is focusable`);
      assert.ok(source.includes("focus()"), `${rel} moves focus to the heading`);
      assert.match(source, /<h[23]/, `${rel} decision title is a real heading`);
    }
  });

  it("keeps every decision control keyboard-operable with visible pairings", () => {
    for (const rel of NEW_ALL) {
      const source = read(rel);
      if (source.includes("onContinue") || source.includes("onConfirmed") || source.includes("<button") || rel.includes("Decision") || rel.includes("Chooser")) {
        assert.ok(source.includes("<button"), `${rel} uses native buttons`);
      }
    }
    assert.ok(read("src/lib/components/OperationProgress.svelte").includes("<svg"), "progress pairs an icon with text");
  });
});

describe("ordinary-app failure offers only continue or rollback", () => {
  it("names affected apps and offers exactly the two approved answers", () => {
    const source = read("src/lib/components/AppFailureDecision.svelte");
    assert.ok(source.includes("packageId"), "names the affected package");
    assert.ok(source.includes("label"), "names the affected app label");
    assert.match(source, /Continue/, "Continue with available present");
    assert.match(source, /Roll back/, "Roll back present");
    assert.doesNotMatch(source, /override/i, "no override button");
    assert.doesNotMatch(source, /home-confirmed/, "no HOME answer in the ordinary decision");
  });

  it("decides before any HOME change", () => {
    const combined = read("src/lib/components/AppFailureDecision.svelte") + read("src/lib/screens/ApplyScreen.svelte");
    assert.match(combined, /before.*home|home.*later/i, "decision happens before the HOME change");
  });
});

describe("rolled-back copy stays generic with no override", () => {
  it("uses the generic rollback copy and offers no override", () => {
    const combined = read("src/lib/screens/ApplyScreen.svelte") + read("src/lib/components/AppFailureDecision.svelte");
    assert.match(combined, /Review the selection and try again/, "generic rollback copy present");
    assert.doesNotMatch(combined, /cannot continue with incomplete store protection/i, "no store attribution without a backend cause");
    assert.doesNotMatch(combined, /override/i, "rolled-back failure offers no override");
  });

  it("has no store-conditional branch: the backend provides no rollback cause (F1)", () => {
    const source = read("src/lib/screens/ApplyScreen.svelte");
    assert.doesNotMatch(source, /isStoreRollback/, "rolled-back copy no longer branches on a known-cause signal");
    assert.match(source, /Review the selection and try again/, "generic rollback copy present");
  });
});

describe("apply announcements do not clear or repeat (F2)", () => {
  it("guards the mount announce so an empty flow message never wipes the host text", () => {
    const source = read("src/lib/screens/ApplyScreen.svelte");
    assert.match(
      source,
      /if\s*\(flow\.snapshot\.announcement\.length\s*>\s*0\)\s*onAnnounce/,
      "mount announce guarded by length>0",
    );
  });

  it("forwards progress only through the flow callback (no handler-side re-announce)", () => {
    const source = read("src/lib/screens/ApplyScreen.svelte");
    const calls = source.match(/onAnnounce\(/g) ?? [];
    assert.equal(calls.length, 1, "exactly one onAnnounce call site (the guarded mount forward)");
  });
});

describe("HOME chooser fallback waits for verified success", () => {
  it("guides the chooser steps and waits for the explicit confirmation", () => {
    const source = read("src/lib/components/LauncherChooserGuide.svelte");
    assert.ok(source.includes("I chose Unscroll Launcher"), "explicit confirmation present");
    assert.ok(source.includes("Cancel"), "cancel present");
    assert.match(source, /chooser/i, "chooser guidance present");
    assert.ok(source.includes("home-confirmed") || source.includes("onConfirmed"), "confirmation wires to home-confirmed");
    assert.ok(source.includes("home-cancelled") || source.includes("onCancelled"), "cancel wires to home-cancelled");
  });

  it("shows success only after backend verification (no frontend-only completion)", () => {
    const screen = read("src/lib/screens/ApplyScreen.svelte");
    assert.ok(screen.includes("isApplyComplete"), "completion derives from the backend-outcome helper");
    assert.ok(read("src/lib/state/workspace.ts").includes("isApplyComplete"), "helper lives in the state machine");
  });
});

describe("partial protection stays honest", () => {
  it("reports sideload gaps as partial protection, never blocked", () => {
    const source = read("src/lib/components/ProtectionSummary.svelte");
    assert.match(source, /partial protection/i, "partial wording present");
    assert.doesNotMatch(source, /blocked/i, "partial gaps are never called blocked");
  });
});

describe("USB debugging guide stays universal", () => {
  it("covers debugging, Developer Options, and the sensitive-app warning", () => {
    const source = read("src/lib/components/UsbDebuggingGuide.svelte");
    assert.match(source, /USB debugging/, "USB debugging guidance present");
    assert.match(source, /Developer Options/, "Developer Options guidance present");
    for (const word of ["banking", "government", "workplace", "sensitive"]) {
      assert.ok(source.toLowerCase().includes(word), `warning names ${word} apps`);
    }
  });

  it("keeps no app-name list", () => {
    const source = read("src/lib/components/UsbDebuggingGuide.svelte");
    assert.doesNotMatch(source, /WhatsApp|Instagram|TikTok|Gmail|Chrome|Facebook|Revolut/i, "no per-app list");
  });
});

describe("completion gates on the backend outcome", () => {
  it("App renders CompletionScreen only after backend complete", () => {
    const app = read("src/App.svelte");
    assert.ok(app.includes("CompletionScreen"), "App wires the completion view");
    assert.ok(app.includes("isApplyComplete") || app.includes('"complete"') || app.includes("'complete'"), "complete-only guard present");
  });

  it("CompletionScreen bundles the summary and the debugging guide", () => {
    const source = read("src/lib/screens/CompletionScreen.svelte");
    assert.ok(source.includes("ProtectionSummary"), "summary present");
    assert.ok(source.includes("UsbDebuggingGuide"), "debugging guide present");
    assert.match(source, /<h2/, "completion has a heading");
  });
});

describe("transport and secrecy invariants", () => {
  it("new sources touch Rust only through invoke helpers plus DTO types", () => {
    const combined = [...NEW_ALL, "src/lib/state/workspace.ts", "src/App.svelte"].map(read).join("\n");
    assert.doesNotMatch(combined, /from\s+["']@tauri-apps\/api\/core["']/, "no direct invoke imports");
    assert.doesNotMatch(combined, /child_process|execSync|spawn\(/, "no process spawning");
    assert.doesNotMatch(combined, /adb shell/i, "no device commands");
    assert.doesNotMatch(combined, /pm (suspend|unsuspend)/i, "no package commands");
  });

  it("new presentation never renders identity or exit codes", () => {
    for (const rel of NEW_ALL) {
      const source = read(rel);
      assert.doesNotMatch(source, /\{serial\}|\{fingerprint\}/, `${rel} never interpolates identity`);
      assert.doesNotMatch(source, /exit code/i, `${rel} never renders exit codes`);
    }
  });

  it("new components need no icon loading", () => {
    const combined = NEW_ALL.map(read).join("\n");
    assert.doesNotMatch(combined, /loadAppIcon|createIconLoader|iconSrcFor/, "no icon loading in apply UI");
  });

  it("new styles add no decorative motion", () => {
    const combined = NEW_ALL.map(read).join("\n");
    assert.doesNotMatch(combined, /@keyframes/, "no keyframe animation");
    assert.doesNotMatch(combined, /animation:/, "no animation declarations");
  });
});
