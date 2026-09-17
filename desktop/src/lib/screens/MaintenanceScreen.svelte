<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { decodeProgressPayload, listenProgress } from "../api/events.ts";
  import {
    MaintenanceFlow,
    SHOW_BUSY_AFTER_MS,
    canExitMaintenance,
    maintenanceExitBlockReason,
    normalizeProgressPayload,
    type MaintenanceSnapshot,
  } from "../state/workspace.ts";
  import MaintenanceWarning from "../components/MaintenanceWarning.svelte";

  interface Props {
    /** Live maintenance flow built by the host from the inspected phone. */
    flow: MaintenanceFlow;
    /** Bumped by the host on every flow mutation; re-reads the snapshot. */
    tick: number;
    onBack: () => void;
    /** Forwards state changes to the shell's single polite region. */
    onAnnounce: (message: string) => void;
  }

  let { flow, tick, onBack, onAnnounce }: Props = $props();

  const snap: MaintenanceSnapshot = $derived.by(() => {
    void tick;
    return flow.snapshot;
  });
  let showBusy = $state(false);
  let unlisten: (() => void) | null = null;

  const exitReason = $derived(maintenanceExitBlockReason(snap));
  // While the window is still recorded open (open, failed close,
  // disconnect, or disagreeing records), Back stays guarded and only a
  // verified close moves the flow forward.
  const exitBlocked = $derived(!canExitMaintenance(snap));

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

<section class="maintenance" aria-labelledby="maintenance-heading" aria-busy={snap.busy}>
  <h2 id="maintenance-heading">Store maintenance</h2>
  {#if snap.status === "open"}
    <p class="open-state">
      <strong>Store maintenance is open.</strong> The phone stays fully usable
      while the window is open.
    </p>
    <p class="lede">
      New installs are possible until the verified close. Perform updates on the
      phone now, then close maintenance here so the store restrictions are
      re-applied and verified.
    </p>
    {#if exitReason}
      <p class="gate-reason">{exitReason}</p>
    {/if}
    {#if showBusy}
      <p class="busy-row">Closing maintenance…</p>
    {/if}
    <div class="actions">
      <button
        type="button"
        class="u-button-primary"
        disabled={snap.busy}
        onclick={() => {
          void flow.close([], []);
        }}
      >
        Close maintenance
      </button>
    </div>
  {:else if snap.status === "closing" || snap.status === "opening" || snap.status === "verifying"}
    <p class="lede">
      {#if snap.status === "closing"}
        Closing store maintenance and re-applying the store restrictions.
      {:else}
        Checking the connected phone before opening store maintenance.
      {/if}
    </p>
    {#if showBusy}
      <p class="busy-row">Working…</p>
    {/if}
    {#if exitReason}
      <p class="gate-reason">{exitReason}</p>
    {/if}
    <div class="actions">
      <button
        type="button"
        class="u-button-secondary"
        disabled={snap.busy || exitBlocked}
        onclick={onBack}
      >
        Back to policy options
      </button>
    </div>
  {:else if snap.status === "closed"}
    <p class="lede">
      Store maintenance is closed and the store restrictions are verified on the
      phone. New apps outside the allowlist were recorded and restricted again.
    </p>
    <div class="actions">
      <button type="button" class="u-button-secondary" onclick={onBack}>Back to policy options</button>
    </div>
  {:else if snap.status === "recoverable-disconnect"}
    <p class="gate-reason">The phone stopped responding. Check the USB cable, then close maintenance again.</p>
    {#if exitReason}
      <p class="gate-reason">{exitReason}</p>
    {/if}
    <div class="actions">
      {#if snap.opened}
        <button
          type="button"
          class="u-button-primary"
          disabled={snap.busy}
          onclick={() => {
            void flow.close([], []);
          }}
        >
          Close maintenance
        </button>
      {/if}
      <button
        type="button"
        class="u-button-secondary"
        disabled={snap.busy || exitBlocked}
        onclick={onBack}
      >
        Back to policy options
      </button>
    </div>
  {:else if snap.status === "inconsistent-state"}
    <p class="gate-reason">The phone records disagree, so no automatic change is safe.</p>
    {#if exitReason}
      <p class="gate-reason">{exitReason}</p>
    {/if}
    <div class="actions">
      {#if snap.opened}
        <button
          type="button"
          class="u-button-primary"
          disabled={snap.busy}
          onclick={() => {
            void flow.close([], []);
          }}
        >
          Close maintenance
        </button>
      {/if}
      <button
        type="button"
        class="u-button-secondary"
        disabled={snap.busy || exitBlocked}
        onclick={onBack}
      >
        Back to policy options
      </button>
    </div>
  {:else if snap.opened}
    <p class="lede">
      Store maintenance is still recorded open. Only a verified close moves
      forward; leaving this view keeps the exit guard in place.
    </p>
    {#if snap.status === "failed" && snap.error}
      <p class="gate-reason">{snap.error.message} {snap.error.action}</p>
    {/if}
    {#if exitReason}
      <p class="gate-reason">{exitReason}</p>
    {/if}
    {#if showBusy}
      <p class="busy-row">Closing maintenance…</p>
    {/if}
    <div class="actions">
      <button
        type="button"
        class="u-button-primary"
        disabled={snap.busy}
        onclick={() => {
          void flow.close([], []);
        }}
      >
        Close maintenance
      </button>
      <button
        type="button"
        class="u-button-secondary"
        disabled={snap.busy || exitBlocked}
        onclick={onBack}
      >
        Back to policy options
      </button>
    </div>
  {:else}
    <p class="lede">
      Maintenance temporarily restores the recorded store state so updates can
      run, then re-applies the restrictions. Nothing opens until the typed
      acknowledgment matches exactly.
    </p>
    {#if snap.status === "failed" && snap.error}
      <p class="gate-reason">{snap.error.message} {snap.error.action}</p>
    {/if}
    <MaintenanceWarning
      busy={snap.busy}
      onOpen={(confirmation) => {
        void flow.open(confirmation);
      }}
    />
    {#if exitReason}
      <p class="gate-reason">{exitReason}</p>
    {/if}
    <div class="actions">
      <button
        type="button"
        class="u-button-secondary"
        disabled={snap.busy || exitBlocked}
        onclick={onBack}
      >
        Back to policy options
      </button>
    </div>
  {/if}
</section>

<style>
  .maintenance {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
  }

  .maintenance h2 {
    margin: 0;
    font-size: 24px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }

  .lede {
    margin: 0;
    max-width: 72ch;
    color: var(--unscroll-muted);
  }

  .open-state {
    margin: 0;
    max-width: 72ch;
    font-size: 17px;
  }

  .gate-reason {
    margin: 0;
    font-weight: 600;
  }

  .busy-row {
    margin: 0;
    font-weight: 600;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }
</style>
