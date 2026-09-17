<script lang="ts">
  import type { AppEntryDto } from "../api/types.ts";
  import {
    applyBlockReason,
    canApply,
    reviewGroups,
    type ConnectionState,
    type SelectionMap,
    type SessionDto,
  } from "../state/workspace.ts";
  import ReviewGroup from "../components/ReviewGroup.svelte";

  interface Props {
    entries: AppEntryDto[];
    selection: SelectionMap;
    connection: ConnectionState;
    session: SessionDto | null;
    onBack: () => void;
    /** Task 18 wires the real apply flow; review only requests it. */
    onApply: () => void;
  }

  let { entries, selection, connection, session, onBack, onApply }: Props = $props();

  const groups = $derived(reviewGroups(entries, selection));
  const allowed = $derived(canApply({ connection, session, entries }));
  const blockReason = $derived(applyBlockReason(connection, session, entries));

  const baselineNote = $derived(
    groups.baselineLauncher
      ? "The baseline launcher stays installed and unsuspended for recents and gesture navigation; hidden from Unscroll Launcher."
      : null,
  );
</script>

<section class="review" aria-labelledby="review-heading">
  <h2 id="review-heading">Review your choices</h2>
  <p class="lede">
    Check the complete effect before anything changes. Moving between Choose and
    Review never changes the phone.
  </p>

  <ReviewGroup
    title="Apps that will remain available"
    description="These kept apps appear in Unscroll Launcher."
    entries={groups.kept}
    pill="kept"
    emptyText="No apps kept. Keep at least the apps you need every day."
  />
  <ReviewGroup
    title="Apps that will be suspended and hidden"
    description="These apps stay installed with their data, but cannot launch normally and stop sending notifications where suspension is supported."
    entries={groups.blocked}
    pill="blocked"
    emptyText="No apps will be blocked."
  />
  <ReviewGroup
    title="Stores and sideload sources"
    description="These install sources will be restricted so new apps cannot be added outside a store maintenance window."
    entries={groups.stores}
    pill="store"
    emptyText="No stores or sideload sources detected."
  />
  <ReviewGroup
    title="Protected apps"
    description="These system apps cannot be blocked. They stay available to keep the phone working."
    note={baselineNote}
    entries={groups.protected}
    pill="protected"
    emptyText="No protected apps reported."
  />
  <ReviewGroup
    title="Unsupported or manufacturer-protected items"
    description="These items cannot safely be blocked and may remain usable after setup."
    entries={groups.unsupported}
    pill="unsupported"
    emptyText="No unsupported items."
  />

  <p class="preflight-note">
    Apply stays disabled until the phone is inspected for a new setup with at
    least one app. The apply step re-checks the live connection before
    changing anything. Applying suspends blocked apps, restricts stores, and
    switches the default launcher.
  </p>
  {#if !allowed && blockReason}
    <p class="apply-reason">{blockReason}</p>
  {/if}

  <div class="actions">
    <button type="button" class="u-button-secondary" onclick={onBack}>Back to Choose</button>
    <button type="button" class="u-button-primary" disabled={!allowed} onclick={onApply}>Apply</button>
  </div>
</section>

<style>
  .review {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
  }

  .review h2 {
    margin: 0;
    font-size: 24px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }

  .lede,
  .preflight-note {
    margin: 0;
    max-width: 72ch;
    color: var(--unscroll-muted);
  }

  .apply-reason {
    margin: 0;
    font-weight: 600;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }
</style>
