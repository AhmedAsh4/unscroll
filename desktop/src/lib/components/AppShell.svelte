<script lang="ts">
  import type { Snippet } from "svelte";
  import type { StepState } from "../state/workspace.ts";
  import StepRail from "./StepRail.svelte";
  import TitleBar from "./TitleBar.svelte";

  interface Props {
    steps: StepState[];
    /** Connection pill state shown in the title bar. */
    statusState: "ok" | "busy" | "error" | "idle";
    statusText: string;
    /** Single polite announcement; rendered once in the shell live region. */
    announcement: string;
    busy: boolean;
    children?: Snippet;
  }

  let { steps, statusState, statusText, announcement, busy, children }: Props = $props();
</script>

<div class="u-shell">
  <a class="skip" href="#unscroll-main">Skip to main content</a>
  <header class="u-bar">
    <TitleBar statusState={statusState} {statusText} />
  </header>
  <div class="u-body">
    <nav class="u-rail" aria-label="Setup steps">
      <StepRail {steps} />
    </nav>
    <main id="unscroll-main" class="u-main" tabindex="-1" aria-busy={busy}>
      <div class="u-main-inner">
        {@render children?.()}
      </div>
    </main>
  </div>
  <div class="sr-only" role="status" aria-live="polite">{announcement}</div>
</div>

<style>
  .u-shell {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--unscroll-surface);
    color: var(--unscroll-text);
  }

  .skip {
    position: absolute;
    left: 12px;
    top: -48px;
    z-index: 10;
    background: var(--unscroll-card);
    color: var(--unscroll-text);
    border: 1px solid var(--unscroll-border);
    border-radius: var(--unscroll-radius-small);
    padding: 8px 14px;
    font-weight: 600;
    text-decoration: none;
    transition: top 120ms ease;
  }

  .skip:focus-visible {
    top: 8px;
  }

  .u-bar {
    height: var(--unscroll-bar-height);
    flex: none;
    background: var(--unscroll-card);
    border-bottom: 1px solid var(--unscroll-border);
  }

  .u-body {
    flex: 1 1 auto;
    display: flex;
    align-items: stretch;
    min-height: 0;
    min-width: 0;
  }

  .u-rail {
    flex: none;
    width: var(--unscroll-rail-width);
    background: var(--unscroll-sidebar);
    color: #ffffff;
    min-height: 0;
  }

  .u-main {
    flex: 1 1 auto;
    min-width: 0;
    padding: 28px 32px 48px;
  }

  .u-main-inner {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    max-width: var(--unscroll-content-max);
    margin: 0 auto;
    min-width: 0;
  }

  /* Narrow widths (down to the 800px minimum): collapse the rail to a
     compact icon column instead of introducing horizontal scrolling.
     Labels collapse visually but stay in the accessibility tree (no
     display: none, which would also hide names from screen readers). */
  @media (max-width: 800px) {
    .u-rail {
      width: var(--unscroll-rail-collapsed);
    }

    .u-rail :global(.step-label),
    .u-rail :global(.step-note) {
      position: absolute;
      width: 1px;
      height: 1px;
      margin: -1px;
      padding: 0;
      overflow: hidden;
      clip: rect(0 0 0 0);
      white-space: nowrap;
      border: 0;
    }

    .u-main {
      padding: 20px 16px 40px;
    }
  }
</style>
