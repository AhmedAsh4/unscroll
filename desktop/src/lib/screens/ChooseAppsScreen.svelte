<script lang="ts">
  import type { AppEntryDto } from "../api/types.ts";
  import {
    countText,
    filterApps,
    selectionCounts,
    type SelectionFilter,
    type SelectionMap,
  } from "../state/workspace.ts";
  import AppToolbar from "../components/AppToolbar.svelte";
  import AppList from "../components/AppList.svelte";
  import CountCard from "../components/CountCard.svelte";

  interface Props {
    entries: AppEntryDto[];
    selection: SelectionMap;
    onToggle: (packageId: string) => void;
    /** Resolves a local icon URL per entry; null renders the neutral fallback. */
    iconSrcFor?: (entry: AppEntryDto) => string | null;
    onBack: () => void;
    onContinue: () => void;
    /** Single announcement per count change (shell renders the live region). */
    onAnnounce?: (message: string) => void;
  }

  let { entries, selection, onToggle, iconSrcFor = () => null, onBack, onContinue, onAnnounce }: Props = $props();

  let query = $state("");
  let filter: SelectionFilter = $state("all");

  const filtered = $derived(filterApps(entries, selection, query, filter));
  const counts = $derived(selectionCounts(selection, entries));

  // Announce kept/blocked count changes exactly once each; skip the first
  // render so mounting the screen stays silent.
  let lastCountText: string | null = $state(null);
  $effect(() => {
    const text = countText(counts.kept, counts.blocked, counts.total);
    if (lastCountText !== null && text !== lastCountText) {
      onAnnounce?.(text);
    }
    lastCountText = text;
  });
</script>

<section class="choose" aria-labelledby="choose-heading">
  <h2 id="choose-heading">Choose apps to keep</h2>
  <p class="lede">
    Kept apps stay available. Everything else that can safely be blocked starts blocked.
    Protected system apps stay selected and locked with the reason shown. Nothing on the
    phone changes until you review and apply.
  </p>
  <p class="baseline-note">
    The baseline launcher stays installed and unsuspended for recents and gesture
    navigation. It is hidden from Unscroll Launcher and cannot be changed here.
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

  <div class="actions">
    <button type="button" class="u-button-secondary" onclick={onBack}>Back</button>
    <button type="button" class="u-button-primary" onclick={onContinue}>Continue to Review</button>
  </div>
</section>

<style>
  .choose {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
  }

  .choose h2 {
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

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }
</style>
