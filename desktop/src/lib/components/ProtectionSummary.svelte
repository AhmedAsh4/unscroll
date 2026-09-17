<script lang="ts">
  interface Props {
    /** Optional sideload gaps from the backend, shown as partial protection. */
    items: string[];
  }

  let { items }: Props = $props();
</script>

<section class="summary" aria-label="Protection summary">
  <h3 class="summary-title">Protection summary</h3>
  {#if items.length === 0}
    <p class="row">
      <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" focusable="false">
        <circle cx="8" cy="8" r="6.5" fill="none" stroke="currentColor" stroke-width="2" />
        <path d="M5.2 8.4 7.2 10.4 11 6" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
      <span>All requested protections are in place and verified on the phone.</span>
    </p>
  {:else}
    <p class="row">
      <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" focusable="false">
        <path d="M8 2 14.5 13.5H1.5Z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" />
        <line x1="8" y1="6.4" x2="8" y2="10" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />
        <circle cx="8" cy="12" r="1" fill="currentColor" />
      </svg>
      <span>
        Partial protection is in place for these install paths. Each one below
        remains available and needs care when adding apps:
      </span>
    </p>
    <ul class="gaps">
      {#each items as item (item)}
        <li>Partial protection: {item}</li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .summary {
    background: var(--unscroll-card);
    border: 1px solid var(--unscroll-border);
    border-radius: var(--unscroll-radius-small);
    padding: 14px 16px;
    min-width: 0;
  }

  .summary-title {
    margin: 0 0 8px;
    font-size: 15px;
    font-weight: 700;
  }

  .row {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    margin: 0;
    min-width: 0;
  }

  .row svg {
    flex: none;
    margin-top: 1px;
    color: var(--unscroll-sage);
  }

  .gaps {
    margin: 10px 0 0;
    padding-left: 22px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
</style>
