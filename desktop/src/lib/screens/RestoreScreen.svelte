<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { decodeProgressPayload, listenProgress } from "../api/events.ts";
  import { RESTORE_CONFIRMATION } from "../api/types.ts";
  import {
    RestoreFlow,
    SHOW_BUSY_AFTER_MS,
    isRestoreComplete,
    normalizeProgressPayload,
    type RestoreSnapshot,
  } from "../state/workspace.ts";
  import TypedConfirmation from "../components/TypedConfirmation.svelte";
  import RestoreSummary from "../components/RestoreSummary.svelte";
  import OperationProgress from "../components/OperationProgress.svelte";
  import LauncherChooserGuide from "../components/LauncherChooserGuide.svelte";

  interface Props {
    /** Live restore flow built by the host from the inspected phone. */
    flow: RestoreFlow;
    /** Bumped by the host on every flow mutation; re-reads the snapshot. */
    tick: number;
    /** Recorded journal changes the restore will undo, in reverse order. */
    changes: string[];
    onBack: () => void;
    onOpenDiagnostics: () => void;
    /** Forwards state changes to the shell's single polite region. */
    onAnnounce: (message: string) => void;
  }

  let { flow, tick, changes, onBack, onOpenDiagnostics, onAnnounce }: Props = $props();

  const snap: RestoreSnapshot = $derived.by(() => {
    void tick;
    return flow.snapshot;
  });
  let showBusy = $state(false);
  let unlisten: (() => void) | null = null;

  const currentText = $derived.by(() => {
    switch (snap.status) {
      case "verifying":
        return "Checking the connected phone before changing anything.";
      case "running":
        return "Restoring the recorded changes in reverse order and verifying each one.";
      case "chooser-required":
        return "Waiting for the launcher chooser on the phone.";
      case "cleanup-retry":
        return "Restored except final cleanup.";
      case "complete":
        return "Verified complete.";
      case "blocked":
        return "Stopped: restore cannot proceed safely.";
      case "recoverable-disconnect":
        return "Stopped: the phone disconnected.";
      case "inconsistent-state":
        return "Stopped: the phone records disagree.";
      case "failed":
        return "Restore could not proceed.";
      default:
        return "Ready to restore the recorded changes.";
    }
  });

  const recoveryText = $derived.by((): string | null => {
    switch (snap.status) {
      case "cleanup-retry":
        return "Everything is restored except final cleanup. Retry cleanup to remove the remaining recovery data.";
      case "blocked":
        return "Reconnect, reconcile the session, then retry restore.";
      case "recoverable-disconnect":
        return "Check the USB cable, keep the phone awake and unlocked, then restore again.";
      case "inconsistent-state":
        return "Reconnect and reconcile the session before any other action.";
      case "failed":
        return snap.error ? `${snap.error.message} ${snap.error.action}` : "Try again.";
      default:
        return null;
    }
  });

  const isFailed = $derived(
    snap.status === "blocked" ||
      snap.status === "recoverable-disconnect" ||
      snap.status === "inconsistent-state" ||
      snap.status === "failed",
  );

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

<section class="restore" aria-labelledby="restore-heading" aria-busy={snap.busy}>
  <h2 id="restore-heading">Restore phone</h2>
  {#if snap.status === "idle" || snap.status === "failed"}
    <p class="lede">
      Restore undoes every recorded change, restores the previous default
      launcher, verifies the restored state, and removes the recovery data
      last. Nothing restores until the typed confirmation matches exactly.
    </p>
    <RestoreSummary {changes} />
    {#if snap.status === "failed" && snap.error}
      <p class="gate-reason">{snap.error.message} {snap.error.action}</p>
    {/if}
    <TypedConfirmation
      expected={RESTORE_CONFIRMATION}
      busy={snap.busy}
      confirmLabel={snap.status === "failed" ? "Try restore again" : "Restore now"}
      hint="Typing RESTORE MY PHONE confirms the whole recorded policy is undone."
      onConfirm={(value) => {
        void flow.start(value);
      }}
    />
    <div class="actions">
      <button type="button" class="u-button-secondary" disabled={snap.busy} onclick={onBack}>
        Back to policy options
      </button>
    </div>
  {:else if snap.status === "complete"}
    <OperationProgress
      current={currentText}
      verified={snap.restoredCount}
      rollbackSteps={0}
      recovery={null}
      failed={false}
    />
    <p class="lede">
      Restore is complete and verified on the phone. The recovery data is
      removed. Only the backend verification marks this complete.
    </p>
    <div class="actions">
      <button type="button" class="u-button-secondary" onclick={onBack}>Back to policy options</button>
    </div>
  {:else}
    <RestoreSummary {changes} />
    <OperationProgress
      current={currentText}
      verified={snap.restoredCount}
      rollbackSteps={0}
      recovery={recoveryText}
      failed={isFailed}
    />
    {#if showBusy && snap.busy}
      <p class="busy-row">Restoring…</p>
    {/if}
    {#if snap.status === "verifying" || snap.status === "running"}
      <div class="actions">
        <button type="button" class="u-button-secondary" disabled={snap.busy} onclick={onBack}>
          Back to policy options
        </button>
      </div>
    {:else if snap.status === "chooser-required"}
      <LauncherChooserGuide
        busy={snap.busy}
        onConfirmed={() => {
          void flow.answerChooser("home-confirmed");
        }}
        onCancelled={() => {
          void flow.answerChooser("home-cancelled");
        }}
      />
    {:else if snap.status === "cleanup-retry"}
      <p class="lede">
        Everything is restored except final cleanup. Retry cleanup to remove the
        remaining recovery data; restored settings are not repeated.
      </p>
      <div class="actions">
        <button
          type="button"
          class="u-button-primary"
          disabled={snap.busy}
          onclick={() => {
            void flow.retry();
          }}
        >
          Retry cleanup
        </button>
        <button type="button" class="u-button-secondary" disabled={snap.busy} onclick={onBack}>
          Back to policy options
        </button>
      </div>
    {:else if isFailed}
      <div class="actions">
        <button type="button" class="u-button-secondary" disabled={snap.busy} onclick={onBack}>
          Back to policy options
        </button>
        <button type="button" class="u-button-secondary" disabled={snap.busy} onclick={onOpenDiagnostics}>
          Review diagnostics
        </button>
      </div>
    {/if}
    {#if isRestoreComplete(snap)}
      <p class="lede">Verified complete.</p>
    {/if}
  {/if}
</section>

<style>
  .restore {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
  }

  .restore h2 {
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
