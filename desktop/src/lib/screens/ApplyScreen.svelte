<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { decodeProgressPayload, listenProgress } from "../api/events.ts";
  import {
    ApplyFlow,
    SHOW_BUSY_AFTER_MS,
    applyBlockReason,
    blockingAppFor,
    canApply,
    isApplyComplete,
    normalizeProgressPayload,
    type ApplySnapshot,
    type ConnectionState,
    type SessionDto,
  } from "../state/workspace.ts";
  import type { AppEntryDto } from "../api/types.ts";
  import OperationProgress from "../components/OperationProgress.svelte";
  import AppFailureDecision from "../components/AppFailureDecision.svelte";
  import LauncherChooserGuide from "../components/LauncherChooserGuide.svelte";

  interface Props {
    /** Live apply flow built by the host from the inspected phone. */
    flow: ApplyFlow;
    /** Bumped by the host on every flow mutation; re-reads the snapshot. */
    tick: number;
    entries: AppEntryDto[];
    session: SessionDto | null;
    connection: ConnectionState;
    onComplete: () => void;
    onBack: () => void;
    /** Forwards state changes to the shell's single polite region. */
    onAnnounce: (message: string) => void;
  }

  let { flow, tick, entries, session, connection, onComplete, onBack, onAnnounce }: Props = $props();

  // The progress listener lives here (not in the connect view, which
  // unmounts on navigation), so a cable pull during apply invalidates this
  // flow instead of going unseen. The snapshot re-reads on every host tick
  // because the flow object itself is not deeply reactive.
  const snap: ApplySnapshot = $derived.by(() => {
    void tick;
    return flow.snapshot;
  });
  let showBusy = $state(false);
  let completedNotified = $state(false);
  let unlisten: (() => void) | null = null;

  const gateOk = $derived(canApply({ connection, session, entries }));
  const gateReason = $derived(applyBlockReason(connection, session, entries));

  const failedApps = $derived.by(() => {
    const found = blockingAppFor(entries, snap.blockingPackage);
    return found ? [found] : [];
  });

  const currentText = $derived.by(() => {
    switch (snap.status) {
      case "verifying":
        return "Checking the connected phone before changing anything.";
      case "running":
        return "Applying protections in order and verifying each one.";
      case "rolling-back":
        return "Rolling back the completed changes in reverse order.";
      case "decision-required":
        return "Paused for your decision; the default launcher has not changed.";
      case "chooser-required":
        return "Waiting for the launcher chooser on the phone.";
      case "rolled-back":
        return "Rolled back and verified.";
      case "recoverable-disconnect":
        return "Stopped: the phone disconnected.";
      case "inconsistent-state":
        return "Stopped: the phone records disagree.";
      case "failed":
        return "Apply could not proceed.";
      case "complete":
        return "Verified complete.";
      default:
        return "Ready to apply the reviewed policy.";
    }
  });

  const recoveryText = $derived.by((): string | null => {
    switch (snap.status) {
      case "rolled-back":
        // The backend envelope carries no rollback cause (direct rolled-back
        // also occurs for LauncherPolicy/Verify/prior-session resume), so
        // always use the generic copy. Mirror of rolledBackRecoveryText in
        // workspace.ts (kept inline so the copy stays reviewable here).
        return "All completed changes were rolled back and verified; the phone is unchanged. Review the selection and try again.";
      case "recoverable-disconnect":
        return "Check the USB cable, keep the phone awake and unlocked, then go back and apply again.";
      case "inconsistent-state":
        return "Reconnect and reconcile the session before any other action.";
      case "failed":
        return snap.error ? snap.error.action : "Try again.";
      default:
        return null;
    }
  });

  const isFailed = $derived(
    snap.status === "rolled-back" ||
      snap.status === "recoverable-disconnect" ||
      snap.status === "inconsistent-state" ||
      snap.status === "failed",
  );

  const busyText = $derived(
    snap.status === "verifying" ? "Checking the phone…" : "Applying protections…",
  );

  function startNow(): void {
    void flow.start();
  }

  $effect(() => {
    if (!completedNotified && isApplyComplete(snap)) {
      completedNotified = true;
      onComplete();
    }
  });

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
    // Guarded mount announce: a fresh flow holds "", and forwarding it
    // would wipe the host's "Confirm the apply step..." announcement.
    if (flow.snapshot.announcement.length > 0) onAnnounce(flow.snapshot.announcement);
    void listenProgress((payload) => {
      const decoded = decodeProgressPayload(payload as unknown);
      if (normalizeProgressPayload(decoded) === null) return;
      // No handler-side announce: ApplyFlow.say already forwards each real
      // state change exactly once through the host announce callback wired
      // in App.svelte. Announcing here as well double-announces and
      // re-emits stale text on count-only events like operation-applied.
      flow.noteRemoteProgress(decoded);
    }).then((stop) => {
      unlisten = stop;
    });
  });

  onDestroy(() => {
    unlisten?.();
  });
</script>

<section class="apply" aria-labelledby="apply-heading" aria-busy={snap.busy}>
  <h2 id="apply-heading">Apply the policy</h2>
  {#if !gateOk}
    <p class="lede">
      This step re-checks the inspected phone before changing anything. The
      apply view is reachable only through Review, and it validates the same
      gate again here.
    </p>
    {#if gateReason}
      <p class="gate-reason">{gateReason}</p>
    {/if}
    <div class="actions">
      <button type="button" class="u-button-secondary" onclick={onBack}>Back to Review</button>
    </div>
  {:else if snap.status === "idle" || snap.status === "failed"}
    <p class="lede">
      Applying suspends the reviewed apps, restricts install sources, verifies
      every change on the phone, and switches the default launcher last. Start
      when the phone is connected and awake.
    </p>
    {#if snap.status === "failed" && snap.error}
      <p class="gate-reason">{snap.error.message} {snap.error.action}</p>
    {/if}
    <OperationProgress
      current={currentText}
      verified={snap.appliedCount}
      rollbackSteps={snap.rollbackCount}
      recovery={recoveryText}
      failed={snap.status === "failed"}
    />
    {#if showBusy}
      <p class="busy-row">{busyText}</p>
    {/if}
    <div class="actions">
      <button type="button" class="u-button-secondary" disabled={snap.busy} onclick={onBack}>
        Back to Review
      </button>
      <button type="button" class="u-button-primary" disabled={snap.busy} onclick={startNow}>
        {snap.status === "failed" ? "Try again" : "Apply now"}
      </button>
    </div>
  {:else}
    <OperationProgress
      current={currentText}
      verified={snap.appliedCount}
      rollbackSteps={snap.rollbackCount}
      recovery={recoveryText}
      failed={isFailed}
    />
    {#if showBusy && snap.busy}
      <p class="busy-row">{busyText}</p>
    {/if}
    {#if snap.status === "verifying" || snap.status === "running" || snap.status === "rolling-back"}
      <div class="actions">
        <button type="button" class="u-button-secondary" disabled={snap.busy} onclick={onBack}>
          Back to Review
        </button>
      </div>
    {:else if snap.status === "decision-required"}
      <AppFailureDecision
        apps={failedApps}
        busy={snap.busy}
        onContinue={() => {
          void flow.answerDecision("continue");
        }}
        onRollback={() => {
          void flow.answerDecision("rollback");
        }}
      />
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
    {:else if snap.status === "rolled-back"}
      <!-- The backend provides no rollback cause, so always the generic copy. -->
      <p class="lede">
        All completed changes were rolled back and verified; the phone is
        unchanged. Review the selection and try again.
      </p>
      <div class="actions">
        <button type="button" class="u-button-secondary" disabled={snap.busy} onclick={onBack}>
          Back to Review
        </button>
      </div>
    {:else if snap.status === "recoverable-disconnect"}
      <p class="lede">
        The phone stopped responding during apply. Completed changes stay
        journaled for a safe resume or rollback. Check the cable, then go back
        and apply again.
      </p>
      <div class="actions">
        <button type="button" class="u-button-secondary" disabled={snap.busy} onclick={onBack}>
          Back to Review
        </button>
      </div>
    {:else if snap.status === "inconsistent-state"}
      <p class="lede">
        The phone records disagree, so no automatic change is safe. Reconnect
        and reconcile the session before any other action.
      </p>
      <div class="actions">
        <button type="button" class="u-button-secondary" disabled={snap.busy} onclick={onBack}>
          Back to Review
        </button>
      </div>
    {/if}
  {/if}
</section>

<style>
  .apply {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
  }

  .apply h2 {
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
