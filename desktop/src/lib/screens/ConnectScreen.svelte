<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { discoverDevices, inspectDevice, getSession } from "../api/invoke.ts";
  import { listenProgress } from "../api/events.ts";
  import {
    CONNECTION_COPY,
    ConnectionFlow,
    SHOW_BUSY_AFTER_MS,
    deriveSteps,
    routeForSession,
    type InspectedInfo,
    type ShellUpdate,
    type WorkspaceSnapshot,
  } from "../state/workspace.ts";
  import ConnectionStatus from "../components/ConnectionStatus.svelte";
  import DeviceCard from "../components/DeviceCard.svelte";
  import InlineNotice from "../components/InlineNotice.svelte";

  interface Props {
    onState: (update: ShellUpdate) => void;
    /** Continue navigates to the chooser (Task 17); absent keeps the ready card. */
    onContinue?: () => void;
    /** Hands inspected entries/session up once per ready inspection. */
    onInspected?: (info: InspectedInfo) => void;
    /** Restored inspection shown without re-running discovery (Back from chooser). */
    initialSnapshot?: WorkspaceSnapshot | null;
    /** Set false with initialSnapshot to skip auto-connect on mount. */
    autoStart?: boolean;
    /** Observes snapshot changes so the host can preserve them across remounts. */
    onSnapshot?: (snapshot: WorkspaceSnapshot) => void;
  }

  let { onState, onContinue, onInspected, initialSnapshot = null, autoStart = true, onSnapshot }: Props = $props();

  const flow = new ConnectionFlow(
    { discoverDevices, inspectDevice, getSession },
    {
      announce: (message) => {
        announcement = message;
        emit();
      },
      onChange: () => {
        snap = flow.snapshot;
        onSnapshot?.(flow.snapshot);
        emit();
      },
    },
  );

  let snap = $state(flow.snapshot);
  let announcement = $state("");
  let showBusy = $state(false);
  let continued = $state(false);
  // A restored ready snapshot was already handed up before the remount.
  let inspectedNotified = $state(false);
  let unlisten: (() => void) | null = null;

  const statusState = $derived.by(() => {
    if (snap.busy || snap.connection === "checking" || snap.connection === "inspecting" || snap.connection === "connected") {
      return "busy" as const;
    }
    if (snap.connection === "ready") return "ok" as const;
    if (snap.connection === "idle") return "idle" as const;
    return "error" as const;
  });

  const statusText = $derived.by(() => {
    if (statusState === "busy") return "Checking phone";
    if (statusState === "ok") return snap.device?.model ?? "Connected";
    if (statusState === "idle") return "No phone connected";
    return snap.guidance.heading;
  });

  const busyText = $derived(snap.connection === "inspecting" ? "Inspecting the phone…" : "Checking for a phone…");

  const canRetry = $derived(
    !snap.busy &&
      snap.connection !== "checking" &&
      snap.connection !== "inspecting" &&
      snap.connection !== "connected" &&
      snap.connection !== "busy-transaction" &&
      snap.connection !== "ready",
  );

  // Passive-wait states advertise no retryable action: the adjacent control
  // is a visibly disabled "Waiting…" button, not a missing action.
  const isPassiveWait = $derived(
    snap.connection === "busy-transaction" ||
      snap.connection === "checking" ||
      snap.connection === "inspecting" ||
      snap.connection === "connected",
  );

  const platformLabel = $derived(snap.device?.api !== null && snap.device?.api !== undefined ? `Android API ${snap.device.api}` : null);

  const sessionRoute = $derived(snap.session ? routeForSession(snap.session.kind) : null);

  function emit(): void {
    onState({
      steps: deriveSteps(snap.connection, snap.session),
      announcement,
      busy: snap.busy,
      statusState,
      statusText,
    });
  }

  function recheck(): void {
    continued = false;
    inspectedNotified = false;
    flow.reset();
    snap = flow.snapshot;
    void flow.connect();
  }

  function proceed(): void {
    continued = true;
    announcement = `${CONNECTION_COPY.ready.heading}. ${CONNECTION_COPY.ready.body}`;
    emit();
    onContinue?.();
  }

  // Hand inspected data up once per ready inspection so App can seed the
  // shared chooser selection without re-reading device state. A restored
  // ready snapshot was already handed up before this mount, so it is
  // skipped here (the per-mount prop read is stable inside this effect).
  $effect(() => {
    if (snap.connection === "ready" && !inspectedNotified && initialSnapshot?.connection !== "ready") {
      inspectedNotified = true;
      onInspected?.({ entries: snap.entries, session: snap.session, connection: snap.connection });
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
    emit();
    void listenProgress((payload) => {
      flow.noteRemoteProgress(payload as unknown);
      snap = flow.snapshot;
      emit();
    }).then((stop) => {
      unlisten = stop;
    });
    if (initialSnapshot) {
      // Back from the chooser: show the last inspection as-is. Only an
      // explicit user recheck runs discovery again.
      flow.restoreSnapshot(initialSnapshot);
      snap = flow.snapshot;
      onSnapshot?.(flow.snapshot);
      emit();
    } else if (autoStart) {
      void flow.connect();
    }
  });

  onDestroy(() => {
    unlisten?.();
  });
</script>

<section class="connect" aria-labelledby="connect-heading" aria-busy={snap.busy}>
  <h2 id="connect-heading">Connect your phone</h2>
  <p class="lede">
    Plug in one Android phone over USB with USB debugging enabled. Unscroll checks the
    connection, inspects the phone without changing anything, and reconciles any earlier session.
  </p>

  <div class="status-row">
    <ConnectionStatus state={statusState} text={statusText} />
  </div>

  {#if showBusy}
    <p class="busy-row">
      <svg class="spinner" width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" focusable="false">
        <circle cx="8" cy="8" r="6.5" fill="none" stroke="currentColor" stroke-width="2" stroke-dasharray="4 3" />
      </svg>
      {busyText}
    </p>
  {/if}

  {#if snap.device !== null && (snap.device.model !== null || snap.entries.length > 0)}
    <DeviceCard
      model={snap.device.model}
      manufacturer={snap.device.manufacturer}
      platformLabel={platformLabel}
      appCount={snap.entries.length}
    />
  {/if}

  {#if snap.guidance.tone === "error" || snap.connection === "unverified-newer"}
    <InlineNotice
      tone={snap.guidance.tone === "warning" ? "warning" : "error"}
      title={snap.guidance.heading}
      message={snap.guidance.body}
      actionLabel={canRetry ? snap.guidance.action : isPassiveWait ? "Waiting…" : null}
      onAction={canRetry ? recheck : null}
      actionDisabled={!canRetry && isPassiveWait}
    />
  {:else if snap.connection === "ready"}
    <InlineNotice
      tone="info"
      title={snap.guidance.heading}
      message={snap.guidance.body}
      actionLabel={snap.guidance.action}
      onAction={continued ? null : proceed}
      actionDisabled={continued}
      primary
    />
    {#if sessionRoute}
      <InlineNotice tone="info" title={sessionRoute.title} message={sessionRoute.body} />
    {/if}
  {:else if snap.connection === "idle"}
    <button type="button" class="u-button-primary" onclick={recheck}>Check for phone</button>
  {/if}
</section>

<style>
  .connect {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
  }

  .connect h2 {
    margin: 0;
    font-size: 24px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }

  .lede {
    margin: 0;
    max-width: 64ch;
    color: var(--unscroll-muted);
  }

  .status-row {
    display: flex;
  }

  .busy-row {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0;
    font-weight: 600;
  }

  .spinner {
    color: var(--unscroll-sage);
  }
</style>
