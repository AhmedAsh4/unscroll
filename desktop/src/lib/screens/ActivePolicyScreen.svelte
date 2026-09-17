<script lang="ts">
  import type { AppEntryDto, SessionDto } from "../api/types.ts";
  import {
    EditFlow,
    SHOW_BUSY_AFTER_MS,
    allowlistFor,
    canEditPolicy,
    canRestorePolicy,
    computeEditDelta,
    countText,
    filterApps,
    isPolicyReconciled,
    policyActionsFor,
    policyGateReason,
    requiresCloseFirst,
    routeForSession,
    selectionCounts,
    type ConnectionState,
    type SelectionFilter,
    type SelectionMap,
  } from "../state/workspace.ts";
  import AppToolbar from "../components/AppToolbar.svelte";
  import AppList from "../components/AppList.svelte";
  import CountCard from "../components/CountCard.svelte";
  import RecoveryBlock from "../components/RecoveryBlock.svelte";

  interface Props {
    session: SessionDto | null;
    connection: ConnectionState;
    entries: AppEntryDto[];
    selection: SelectionMap;
    /** Allowlist captured when the policy view opened; the delta reference. */
    activeAllowed: string[];
    /** Resolves a local icon URL per entry; null renders the neutral fallback. */
    iconSrcFor: (entry: AppEntryDto) => string | null;
    onToggle: (packageId: string) => void;
    /** Live edit flow built by the host, or null before the first edit start. */
    editFlow: EditFlow | null;
    /** Bumped by the host on every edit-flow mutation; re-reads the snapshot. */
    editTick: number;
    /** Starts an edit for the given allowlist through the host flow. */
    onStartEdit: (allowed: string[]) => void;
    onOpenMaintenance: () => void;
    onOpenRestore: () => void;
    onOpenDiagnostics: () => void;
    onBack: () => void;
    /** Forwards state changes to the shell's single polite region. */
    onAnnounce: (message: string) => void;
  }

  let {
    session,
    connection,
    entries,
    selection,
    activeAllowed,
    iconSrcFor,
    onToggle,
    editFlow,
    editTick,
    onStartEdit,
    onOpenMaintenance,
    onOpenRestore,
    onOpenDiagnostics,
    onBack,
    onAnnounce,
  }: Props = $props();

  let mode: "menu" | "edit" = $state("menu");
  let query = $state("");
  let filter: SelectionFilter = $state("all");
  let lastCountText: string | null = $state(null);
  let showBusy = $state(false);

  const reconciled = $derived(isPolicyReconciled(connection, session));
  const gateReason = $derived(policyGateReason(connection, session));
  const actions = $derived(session ? policyActionsFor(session.kind) : []);
  const route = $derived(session ? routeForSession(session.kind) : null);
  const closeFirst = $derived(requiresCloseFirst(connection, session));
  const editAllowed = $derived(canEditPolicy(connection, session));
  const restoreAllowed = $derived(canRestorePolicy(connection, session));

  const filtered = $derived(filterApps(entries, selection, query, filter));
  const counts = $derived(selectionCounts(selection, entries));
  const proposed = $derived(allowlistFor(entries, selection));
  const delta = $derived(computeEditDelta(activeAllowed, proposed));

  const editSnap = $derived.by(() => {
    void editTick;
    return editFlow?.snapshot ?? null;
  });

  $effect(() => {
    if (mode !== "edit") return;
    const text = countText(counts.kept, counts.blocked, counts.total);
    if (lastCountText !== null && text !== lastCountText) {
      onAnnounce(text);
    }
    lastCountText = text;
  });

  $effect(() => {
    if (!(editSnap?.busy ?? false)) {
      showBusy = false;
      return;
    }
    const timer = window.setTimeout(() => {
      showBusy = true;
    }, SHOW_BUSY_AFTER_MS);
    return () => window.clearTimeout(timer);
  });

  function openEdit(): void {
    lastCountText = null;
    mode = "edit";
    onAnnounce("Edit allowed apps. The original baseline is preserved; only the listed changes will apply.");
  }
</script>

<section class="policy" aria-labelledby="policy-heading" aria-busy={editSnap?.busy ?? false}>
  <h2 id="policy-heading">Phone policy</h2>
  {#if !reconciled}
    <p class="lede">
      Policy actions unlock after the session reconciles. Reconnect and
      reconcile the session instead of editing or restoring blindly.
    </p>
    {#if gateReason}
      <p class="gate-reason">{gateReason}</p>
    {/if}
    <div class="actions">
      <button type="button" class="u-button-secondary" onclick={onBack}>Back to Connect</button>
    </div>
  {:else if session?.kind === "blocked-inconsistency"}
    <RecoveryBlock kind={session.kind} guidance={session.guidance} onExportDiagnostics={onOpenDiagnostics} />
    <div class="actions">
      <button type="button" class="u-button-secondary" onclick={onBack}>Back to Connect</button>
    </div>
  {:else if mode === "edit"}
    {#if !editAllowed}
      <p class="lede">Editing is not available for this session right now.</p>
      {#if gateReason}
        <p class="gate-reason">{gateReason}</p>
      {/if}
      <div class="actions">
        <button
          type="button"
          class="u-button-secondary"
          onclick={() => {
            mode = "menu";
          }}
        >
          Back to policy options
        </button>
      </div>
    {:else}
      <p class="lede">
        Kept apps stay available. Everything else that can safely be blocked
        starts blocked. Protected system apps stay selected and locked with the
        reason shown.
      </p>
      <p class="baseline-note">
        The original baseline is preserved and never replaced. Only the planned
        changes below will apply; later changes stay journaled so a full restore
        can undo them too.
      </p>
      <AppToolbar
        {query}
        {filter}
        resultCount={filtered.length}
        totalCount={entries.length}
        onQuery={(value) => {
          query = value;
        }}
        onFilter={(value) => {
          filter = value;
        }}
      />
      <CountCard kept={counts.kept} blocked={counts.blocked} total={counts.total} />
      <AppList entries={filtered} {selection} {iconSrcFor} {onToggle} />
      <section class="delta" aria-label="Planned changes">
        <h3 class="delta-title">Planned changes only</h3>
        {#if delta.addedToKeep.length === 0 && delta.addedToBlock.length === 0}
          <p class="delta-empty">No changes yet. Keep or block an app to preview the delta.</p>
        {:else}
          <h4 class="delta-group">Added to keep ({delta.addedToKeep.length})</h4>
          {#if delta.addedToKeep.length === 0}
            <p class="delta-empty">Nothing added to keep.</p>
          {:else}
            <ul class="delta-list">
              {#each delta.addedToKeep as packageId (packageId)}
                <li>{packageId}</li>
              {/each}
            </ul>
          {/if}
          <h4 class="delta-group">Added to block ({delta.addedToBlock.length})</h4>
          {#if delta.addedToBlock.length === 0}
            <p class="delta-empty">Nothing added to block.</p>
          {:else}
            <ul class="delta-list">
              {#each delta.addedToBlock as packageId (packageId)}
                <li>{packageId}</li>
              {/each}
            </ul>
          {/if}
        {/if}
      </section>
      {#if editSnap?.status === "failed" && editSnap.error}
        <p class="gate-reason">{editSnap.error.message} {editSnap.error.action}</p>
      {:else if editSnap?.status === "complete"}
        <p class="lede">
          The allowed-apps change is complete and verified on the phone. The
          original baseline is unchanged.
        </p>
      {:else if editSnap?.status === "recoverable-disconnect"}
        <p class="gate-reason">The phone stopped responding. Check the USB cable, then try again.</p>
      {:else if editSnap?.status === "inconsistent-state"}
        <p class="gate-reason">The phone records disagree, so no automatic change is safe.</p>
      {/if}
      {#if showBusy && (editSnap?.busy ?? false)}
        <p class="busy-row">Applying the edit…</p>
      {/if}
      <div class="actions">
        <button
          type="button"
          class="u-button-secondary"
          disabled={editSnap?.busy ?? false}
          onclick={() => {
            mode = "menu";
          }}
        >
          Back to policy options
        </button>
        <button
          type="button"
          class="u-button-primary"
          disabled={(editSnap?.busy ?? false) ||
            (delta.addedToKeep.length === 0 && delta.addedToBlock.length === 0)}
          onclick={() => onStartEdit(proposed)}
        >
          {editSnap?.status === "failed" ? "Try edit again" : "Apply edit"}
        </button>
      </div>
    {/if}
  {:else}
    {#if route}
      <p class="lede"><strong>{route.title}.</strong> {route.body}</p>
    {/if}
    {#if closeFirst}
      <p class="gate-reason">
        A store maintenance window is still open. Close maintenance first; edit,
        restore, and a new window wait until the close verifies.
      </p>
    {/if}
    <div class="actions">
      {#if actions.includes("edit")}
        <button type="button" class="u-button-primary" onclick={openEdit}>Edit allowed apps</button>
      {/if}
      {#if actions.includes("maintenance")}
        <button type="button" class="u-button-secondary" onclick={onOpenMaintenance}>
          Open store maintenance
        </button>
      {/if}
      {#if actions.includes("close-maintenance")}
        <button type="button" class="u-button-primary" onclick={onOpenMaintenance}>
          Close the open maintenance window
        </button>
      {/if}
      {#if actions.includes("restore")}
        <button type="button" class="u-button-secondary" disabled={!restoreAllowed} onclick={onOpenRestore}>
          Restore phone
        </button>
      {/if}
      {#if actions.includes("retry-cleanup")}
        <button type="button" class="u-button-secondary" onclick={onOpenRestore}>
          Retry final cleanup
        </button>
      {/if}
      {#if actions.includes("resume") || actions.includes("rollback")}
        <p class="lede">
          This session resumes or rolls back through the apply view. Go back and
          continue from the connection screen.
        </p>
      {/if}
      {#if actions.includes("diagnostics") || actions.includes("export-diagnostics")}
        <button type="button" class="u-button-secondary" onclick={onOpenDiagnostics}>
          Review diagnostics
        </button>
      {/if}
    </div>
    <div class="actions">
      <button type="button" class="u-button-secondary" onclick={onBack}>Back to Connect</button>
    </div>
  {/if}
</section>

<style>
  .policy {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
  }

  .policy h2 {
    margin: 0;
    font-size: 24px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }

  .lede,
  .baseline-note {
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

  .delta {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
    background: var(--unscroll-card);
    border: 1px solid var(--unscroll-border);
    border-radius: var(--unscroll-radius-small);
    padding: 14px 16px;
  }

  .delta-title {
    margin: 0;
    font-size: 15px;
    font-weight: 700;
  }

  .delta-group {
    margin: 6px 0 0;
    font-size: 14px;
    font-weight: 700;
  }

  .delta-empty {
    margin: 0;
    color: var(--unscroll-muted);
  }

  .delta-list {
    margin: 0;
    padding-left: 22px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-width: 72ch;
  }
</style>
