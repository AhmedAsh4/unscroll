<script lang="ts">
  import type { SelectionFilter } from "../state/workspace.ts";

  interface Props {
    query: string;
    filter: SelectionFilter;
    resultCount: number;
    totalCount: number;
    onQuery: (query: string) => void;
    onFilter: (filter: SelectionFilter) => void;
  }

  let { query, filter, resultCount, totalCount, onQuery, onFilter }: Props = $props();

  const uid = $props.id();
  const searchId = `app-search-${uid}`;
</script>

<div class="toolbar">
  <div class="search-field">
    <label class="search-label" for={searchId}>Search apps</label>
    <input
      id={searchId}
      class="search-input"
      type="search"
      value={query}
      placeholder="Search by name or package"
      oninput={(event) => onQuery(event.currentTarget.value)}
    />
  </div>
  <div class="filter-field" role="radiogroup" aria-label="Filter apps">
    <label class="filter-option">
      <input type="radio" name="app-filter-{uid}" value="all" checked={filter === "all"} onchange={() => onFilter("all")} />
      All
    </label>
    <label class="filter-option">
      <input type="radio" name="app-filter-{uid}" value="kept" checked={filter === "kept"} onchange={() => onFilter("kept")} />
      Kept
    </label>
    <label class="filter-option">
      <input type="radio" name="app-filter-{uid}" value="blocked" checked={filter === "blocked"} onchange={() => onFilter("blocked")} />
      Blocked
    </label>
  </div>
  <p class="results-count">{resultCount} of {totalCount} apps shown</p>
</div>

<style>
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 12px 16px;
    min-width: 0;
  }

  .search-field {
    flex: 1 1 220px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .search-label {
    font-size: 13px;
    font-weight: 600;
  }

  .search-input {
    font: inherit;
    color: var(--unscroll-text);
    background: var(--unscroll-card);
    border: 1px solid var(--unscroll-border);
    border-radius: var(--unscroll-radius-small);
    padding: 8px 12px;
    min-width: 0;
  }

  .filter-field {
    display: flex;
    gap: 12px;
    align-items: center;
    padding-bottom: 8px;
  }

  .filter-option {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-weight: 600;
    font-size: 14px;
  }

  .results-count {
    margin: 0 0 8px auto;
    font-size: 13px;
    color: var(--unscroll-muted);
  }
</style>
