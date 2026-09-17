<script lang="ts">
  import AppShell from "./lib/components/AppShell.svelte";
  import ConnectScreen from "./lib/screens/ConnectScreen.svelte";
  import ChooseAppsScreen from "./lib/screens/ChooseAppsScreen.svelte";
  import ReviewScreen from "./lib/screens/ReviewScreen.svelte";
  import ApplyScreen from "./lib/screens/ApplyScreen.svelte";
  import CompletionScreen from "./lib/screens/CompletionScreen.svelte";
  import { discoverDevices, loadAppIcon, respondToDecision, startApply } from "./lib/api/invoke.ts";
  import {
    ApplyFlow,
    STEPS,
    allowlistFor,
    applyBlockReason,
    canApply,
    countText,
    createIconLoader,
    createSelection,
    isApplyComplete,
    selectionCounts,
    selectionKeyFor,
    selectRowIcon,
    toggleSelection,
    type ApplyInvokeResult,
    type ConnectionState,
    type InspectedInfo,
    type SelectionMap,
    type SessionDto,
    type ShellUpdate,
    type StepState,
    type WorkspaceSnapshot,
  } from "./lib/state/workspace.ts";
  import type { AppEntryDto } from "./lib/api/types.ts";

  type View = "connect" | "choose" | "review" | "apply" | "complete";

  let steps: StepState[] = $state(STEPS.map((step, index) => ({ ...step, status: index === 0 ? "active" as const : "pending" as const })));
  let announcement = $state("");
  let busy = $state(false);
  let statusState: ShellUpdate["statusState"] = $state("idle");
  let statusText = $state("No phone connected");

  let view: View = $state("connect");
  let entries: AppEntryDto[] = $state([]);
  let selection: SelectionMap = $state({});
  let selectionKey: string | null = $state(null);
  let session: SessionDto | null = $state(null);
  // Last inspection as ConnectScreen left it. The progress listener lives
  // in ConnectScreen, so once it unmounts this snapshot goes stale: it
  // seeds Review's gate only. Task 18 re-checks the live connection
  // before any mutation, so nothing here promises live connection state.
  let inspectedConnection: ConnectionState = $state("idle");
  // Full flow snapshot preserved across Connect remounts so Back from the
  // chooser restores the last inspection instead of re-running discovery.
  let savedSnapshot: WorkspaceSnapshot | null = $state(null);
  // Live apply flow for the Task 18 apply/completion views. Built only
  // through the Review gate in handleApply; identity values stay in memory
  // for invoke arguments only and are never rendered.
  let applyFlow: ApplyFlow | null = $state(null);
  // Bumped by the flow on every mutation so the apply view re-reads the
  // latest snapshot (the flow object itself is not deeply reactive).
  let applyTick = $state(0);

  // Bounded lazy icons: one loader per inspection (cache Map + in-flight
  // dedup, fire once per packageId). The transport reads the current
  // inspected identity at call time; misses/errors resolve to null so rows
  // render the existing neutral fallback. Identity values stay in memory
  // for invoke arguments only — never rendered.
  function makeIconLoader() {
    return createIconLoader(async (packageId: string) => {
      const serial = savedSnapshot?.device?.serial;
      const fingerprint = savedSnapshot?.device?.fingerprint;
      if (!serial || !fingerprint) return null;
      const result = await loadAppIcon(serial, fingerprint, packageId);
      if (!result.ok) return null;
      return result.value;
    });
  }

  let iconLoader = $state(makeIconLoader());
  let iconTick = $state(0);

  function iconSrcFor(entry: AppEntryDto): string | null {
    // Subscribe the row render to loader completions.
    void iconTick;
    // Fresh-catalog precedence lives in selectRowIcon: a current
    // iconCached===false beats any stale loader hit from a previous
    // inspection, so same-set re-inspections never serve dead icons.
    const decision = selectRowIcon(entry, iconLoader.cached(entry.packageId));
    if (decision.kind === "load") {
      void iconLoader.get(entry.packageId).then(() => {
        iconTick += 1;
      });
      return null;
    }
    return decision.src;
  }

  function handleState(update: ShellUpdate): void {
    steps = update.steps;
    announcement = update.announcement;
    busy = update.busy;
    statusState = update.statusState;
    statusText = update.statusText;
  }

  function handleInspected(info: InspectedInfo): void {
    entries = info.entries;
    session = info.session;
    inspectedConnection = info.connection;
    // Every inspection clears the Rust icon cache, so drop this view's
    // cache/flights too — even when the packageId set is unchanged, the
    // bytes (or their absence) may differ. selectRowIcon additionally
    // prefers a current iconCached===false over any lingering hit.
    iconLoader = makeIconLoader();
  }

  function handleSnapshot(snapshot: WorkspaceSnapshot): void {
    savedSnapshot = snapshot;
  }

  function goToChoose(): void {
    // One shared selection: seed from a fresh inspection, but keep the
    // user's toggles across Choose <-> Review back-navigation.
    const key = selectionKeyFor(entries);
    const reseeded = selectionKey !== null && key !== selectionKey;
    if (key !== selectionKey) {
      selection = createSelection(entries);
      selectionKey = key;
      // A new inspection may carry new icons: drop the old cache/flights.
      iconLoader = makeIconLoader();
    }
    view = "choose";
    const counts = selectionCounts(selection, entries);
    announcement = reseeded
      ? `The phone's app list changed since the last inspection, so the selection was reset to defaults. ${countText(counts.kept, counts.blocked, counts.total)}. Choose the apps to keep.`
      : `${countText(counts.kept, counts.blocked, counts.total)}. Choose the apps to keep.`;
  }

  function handleToggle(packageId: string): void {
    const target = entries.find((row) => row.packageId === packageId);
    if (!target) return;
    selection = toggleSelection(selection, target);
  }

  function handleAnnounce(message: string): void {
    announcement = message;
  }

  // Review marks Choose complete and Review active; the apply view marks
  // Choose and Review complete with Apply active; completion marks all
  // complete. Other views reuse the connection-derived steps.
  const viewSteps = $derived.by((): StepState[] => {
    if (view === "review") {
      return steps.map((step): StepState => {
        if (step.id === "choose") return { ...step, status: "complete" };
        if (step.id === "review") return { ...step, status: "active" };
        return step;
      });
    }
    if (view === "apply") {
      return steps.map((step): StepState => {
        if (step.id === "choose" || step.id === "review") return { ...step, status: "complete" };
        if (step.id === "apply") return { ...step, status: "active" };
        return step;
      });
    }
    if (view === "complete") {
      return steps.map((step): StepState => ({ ...step, status: "complete" }));
    }
    return steps;
  });

  function wrapApply(promise: Promise<unknown>): Promise<ApplyInvokeResult> {
    return promise as Promise<ApplyInvokeResult>;
  }

  function handleApply(): void {
    // Real Task 18 flow: Review's gate is re-checked here (and again inside
    // the apply view), the allowlist derives from kept packageIds, and the
    // flow re-checks the live device before any mutation.
    if (!canApply({ connection: inspectedConnection, session, entries })) {
      announcement =
        applyBlockReason(inspectedConnection, session, entries) ??
        "Connect and inspect the phone before applying.";
      return;
    }
    const liveSerial = savedSnapshot?.device?.serial;
    const liveFingerprint = savedSnapshot?.device?.fingerprint;
    if (!liveSerial || !liveFingerprint) {
      announcement = "The inspected phone is no longer available. Check the connection and inspect again.";
      return;
    }
    const allowed = allowlistFor(entries, selection);
    applyFlow = new ApplyFlow(
      { serial: liveSerial, fingerprint: liveFingerprint, allowed, entries },
      {
        discoverDevices: () => wrapApply(discoverDevices()),
        startApply: (serial, fingerprint, allowlist) => wrapApply(startApply(serial, fingerprint, allowlist)),
        respondToDecision: (serial, fingerprint, decision) =>
          wrapApply(respondToDecision(serial, fingerprint, decision)),
      },
      {
        announce: (message) => {
          announcement = message;
        },
        onChange: () => {
          applyTick += 1;
        },
      },
    );
    view = "apply";
    announcement = `${countText(selectionCounts(selection, entries).kept, selectionCounts(selection, entries).blocked, entries.length)}. Confirm the apply step to change the phone.`;
  }

  function handleApplyComplete(): void {
    // Completion renders only after the backend verified outcome.
    if (applyFlow !== null && isApplyComplete(applyFlow.snapshot)) {
      view = "complete";
      announcement = "Setup is complete and verified on the phone.";
    }
  }
</script>

<AppShell steps={viewSteps} {announcement} {busy} {statusState} {statusText}>
  {#if view === "connect"}
    <ConnectScreen
      onState={handleState}
      onContinue={goToChoose}
      onInspected={handleInspected}
      initialSnapshot={savedSnapshot}
      autoStart={savedSnapshot === null}
      onSnapshot={handleSnapshot}
    />
  {:else if view === "choose"}
    <ChooseAppsScreen
      {entries}
      {selection}
      {iconSrcFor}
      onToggle={handleToggle}
      onBack={() => {
        view = "connect";
      }}
      onContinue={() => {
        view = "review";
      }}
      onAnnounce={handleAnnounce}
    />
  {:else if view === "apply"}
    {#if applyFlow !== null}
      <ApplyScreen
        flow={applyFlow}
        tick={applyTick}
        {entries}
        {session}
        connection={inspectedConnection}
        onComplete={handleApplyComplete}
        onBack={() => {
          view = "review";
        }}
        onAnnounce={handleAnnounce}
      />
    {/if}
  {:else if view === "complete"}
    {#if applyFlow !== null}
      <CompletionScreen partialProtection={applyFlow.snapshot.partialProtection} />
    {/if}
  {:else}
    <ReviewScreen
      {entries}
      {selection}
      connection={inspectedConnection}
      {session}
      onBack={() => {
        view = "choose";
      }}
      onApply={handleApply}
    />
  {/if}
</AppShell>
