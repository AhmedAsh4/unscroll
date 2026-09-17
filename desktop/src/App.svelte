<script lang="ts">
  import AppShell from "./lib/components/AppShell.svelte";
  import ConnectScreen from "./lib/screens/ConnectScreen.svelte";
  import ChooseAppsScreen from "./lib/screens/ChooseAppsScreen.svelte";
  import ReviewScreen from "./lib/screens/ReviewScreen.svelte";
  import {
    STEPS,
    countText,
    createSelection,
    selectionCounts,
    selectionKeyFor,
    toggleSelection,
    type ConnectionState,
    type InspectedInfo,
    type SelectionMap,
    type SessionDto,
    type ShellUpdate,
    type StepState,
    type WorkspaceSnapshot,
  } from "./lib/state/workspace.ts";
  import type { AppEntryDto } from "./lib/api/types.ts";

  type View = "connect" | "choose" | "review";

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

  // Review marks Choose complete and Review active; other views reuse the
  // connection-derived steps so the rail keeps working everywhere.
  const viewSteps = $derived.by((): StepState[] => {
    if (view !== "review") return steps;
    return steps.map((step): StepState => {
      if (step.id === "choose") return { ...step, status: "complete" };
      if (step.id === "review") return { ...step, status: "active" };
      return step;
    });
  });

  function handleApply(): void {
    // Task 18 wires the real apply flow; review only requests it.
    announcement = "The guided apply step lands in the next build. Your selection is preserved.";
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
      onToggle={handleToggle}
      onBack={() => {
        view = "connect";
      }}
      onContinue={() => {
        view = "review";
      }}
      onAnnounce={handleAnnounce}
    />
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
