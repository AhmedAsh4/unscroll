<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { decodeProgressPayload, listenProgress } from "../api/events.ts";
  import {
    DiagnosticsFlow,
    SHOW_BUSY_AFTER_MS,
    normalizeProgressPayload,
    type DiagnosticsSnapshot,
  } from "../state/workspace.ts";
  import DiagnosticPreview from "../components/DiagnosticPreview.svelte";

  interface Props {
    /** Live diagnostics flow built by the host from the inspected phone. */
    flow: DiagnosticsFlow;
    /** Bumped by the host on every flow mutation; re-reads the snapshot. */
    tick: number;
    onBack: () => void;
    /** Forwards state changes to the shell's single polite region. */
    onAnnounce: (message: string) => void;
  }

  let { flow, tick, onBack, onAnnounce }: Props = $props();

  const snap: DiagnosticsSnapshot = $derived.by(() => {
    void tick;
    return flow.snapshot;
  });
  let destination = $state("");
  let showBusy = $state(false);
  let unlisten: (() => void) | null = null;

  $effect(() => {
    if (!snap.busy) {
      showBusy = false;
      return;
    }
    const timer = window.setTimeout(() => {
      showBusy = true;
    }, SHOW_BUSY_AFTER_MS);
    return () => window.clearTimeout(timer);
  });

  onMount(() => {
    if (flow.snapshot.announcement.length > 0) onAnnounce(flow.snapshot.announcement);
    void listenProgress((payload) => {
      const decoded = decodeProgressPayload(payload as unknown);
      if (normalizeProgressPayload(decoded) === null) return;
      flow.noteRemoteProgress(decoded);
    }).then((stop) => {
      unlisten = stop;
    });
  });

  onDestroy(() => {
    unlisten?.();
  });
</script>

<section class="diagnostics" aria-labelledby="diagnostics-heading" aria-busy={snap.busy}>
  <h2 id="diagnostics-heading">Diagnostics</h2>
  <p class="lede">
    Preview the redacted report first, then save it to an explicit file
    location on this computer. The export is written locally only; nothing is
    uploaded.
  </p>
  {#if snap.status === "idle" || snap.status === "loading-preview" || (snap.status === "failed" && !snap.preview)}
    {#if snap.status === "failed" && snap.error}
      <p class="gate-reason">{snap.error.message} {snap.error.action}</p>
    {/if}
    <div class="actions">
      <button type="button" class="u-button-primary" disabled={snap.busy} onclick={() => void flow.preview()}>
        Load preview
      </button>
      <button type="button" class="u-button-secondary" disabled={snap.busy} onclick={onBack}>
        Back to policy options
      </button>
    </div>
    {#if showBusy}
      <p class="busy-row">Loading the preview…</p>
    {/if}
  {:else}
    {#if snap.preview}
      <DiagnosticPreview preview={snap.preview} />
    {/if}
    {#if snap.status === "exported"}
      <p class="lede">
        The diagnostic export is saved on this computer. Nothing was uploaded.
      </p>
      <div class="actions">
        <button type="button" class="u-button-secondary" onclick={onBack}>Back to policy options</button>
      </div>
    {:else}
      {#if snap.error}
        <p class="gate-reason">{snap.error.message} {snap.error.action}</p>
      {/if}
      <label class="field-label" for="diagnostics-destination">
        Export destination (absolute file path on this computer)
      </label>
      <input
        id="diagnostics-destination"
        class="field-input"
        type="text"
        autocomplete="off"
        spellcheck="false"
        placeholder="C:\Users\you\Documents\unscroll-diagnostics.json"
        bind:value={destination}
        disabled={snap.busy}
      />
      <p class="field-note">
        Type the full file path, including the folder and the file name. The
        folder must already exist.
      </p>
      {#if showBusy}
        <p class="busy-row">Writing the export…</p>
      {/if}
      <div class="actions">
        <button type="button" class="u-button-secondary" disabled={snap.busy} onclick={onBack}>
          Back to policy options
        </button>
        <button
          type="button"
          class="u-button-primary"
          disabled={snap.busy || destination.trim().length === 0}
          onclick={() => void flow.export(destination)}
        >
          Save export locally
        </button>
      </div>
    {/if}
  {/if}
</section>

<style>
  .diagnostics {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
  }

  .diagnostics h2 {
    margin: 0;
    font-size: 24px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }

  .lede,
  .field-note {
    margin: 0;
    max-width: 72ch;
    color: var(--unscroll-muted);
  }

  .gate-reason {
    margin: 0;
    font-weight: 600;
  }

  .busy-row {
    margin: 0;
    font-weight: 600;
  }

  .field-label {
    font-weight: 600;
  }

  .field-input {
    font: inherit;
    color: var(--unscroll-text);
    background: var(--unscroll-card);
    border: 1px solid var(--unscroll-border);
    border-radius: var(--unscroll-radius-small);
    padding: 10px 12px;
    max-width: 64ch;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }
</style>
