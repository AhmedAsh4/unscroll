<script lang="ts">
  import type { AppEntryDto } from "../api/types.ts";
  import type { PillState } from "../state/workspace.ts";
  import StatePill from "./StatePill.svelte";

  interface Props {
    title: string;
    description: string;
    /** Extra explainer rendered under the description (e.g. baseline launcher). */
    note?: string | null;
    entries: AppEntryDto[];
    pill: PillState;
    emptyText?: string;
  }

  let { title, description, note = null, entries, pill, emptyText = "No apps in this group." }: Props = $props();
</script>

<section class="review-group" aria-label={title}>
  <h3 class="group-title">{title}</h3>
  <p class="group-description">{description}</p>
  {#if note}
    <p class="group-note">{note}</p>
  {/if}
  <p class="group-count">{entries.length} {entries.length === 1 ? "app" : "apps"}</p>
  {#if entries.length === 0}
    <p class="group-empty">{emptyText}</p>
  {:else}
    <ul class="group-list">
      {#each entries as entry (entry.packageId)}
        <li class="group-item">
          <span class="item-text">
            <span class="item-label">{entry.label}</span>
            <span class="item-package">{entry.packageId}</span>
            {#if entry.protectedReason}
              <span class="item-reason">{entry.protectedReason}</span>
            {/if}
          </span>
          <StatePill state={pill} />
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .review-group {
    background: var(--unscroll-card);
    border: 1px solid var(--unscroll-border);
    border-radius: var(--unscroll-radius);
    padding: 16px 18px;
    min-width: 0;
  }

  .group-title {
    margin: 0 0 4px;
    font-size: 16px;
    font-weight: 700;
  }

  .group-description,
  .group-note {
    margin: 0 0 6px;
    font-size: 14px;
    color: var(--unscroll-muted);
    overflow-wrap: break-word;
  }

  .group-count {
    margin: 0 0 10px;
    font-size: 13px;
    font-weight: 700;
  }

  .group-empty {
    margin: 0;
    font-size: 14px;
    color: var(--unscroll-muted);
  }

  .group-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .group-item {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    min-width: 0;
  }

  .item-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .item-label {
    font-weight: 600;
    overflow-wrap: break-word;
  }

  .item-package,
  .item-reason {
    font-size: 12px;
    color: var(--unscroll-muted);
    overflow-wrap: break-word;
  }
</style>
