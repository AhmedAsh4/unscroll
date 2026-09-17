<script lang="ts">
  import type { AppEntryDto } from "../api/types.ts";
  import { pillForEntry, type SelectionMap } from "../state/workspace.ts";
  import AppRow from "./AppRow.svelte";

  interface Props {
    entries: AppEntryDto[];
    selection: SelectionMap;
    iconSrcFor?: (entry: AppEntryDto) => string | null;
    onToggle: (packageId: string) => void;
  }

  let { entries, selection, iconSrcFor = () => null, onToggle }: Props = $props();
</script>

{#if entries.length === 0}
  <p class="empty">No apps match your search or filter.</p>
{:else}
  <ul class="app-list" role="list" aria-label="Apps on the phone">
    {#each entries as entry (entry.packageId)}
      {@const kept = selection[entry.packageId] === true}
      <AppRow entry={entry} {kept} locked={entry.protected} pill={pillForEntry(entry, kept)} iconSrc={iconSrcFor(entry)} {onToggle} />
    {/each}
  </ul>
{/if}

<style>
  .app-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }

  .empty {
    margin: 0;
    padding: 18px;
    text-align: center;
    color: var(--unscroll-muted);
    background: var(--unscroll-card);
    border: 1px solid var(--unscroll-border);
    border-radius: var(--unscroll-radius-small);
  }
</style>
