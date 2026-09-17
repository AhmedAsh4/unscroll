<script lang="ts">
  interface Props {
    /** Recorded journal changes the restore will undo, in reverse order. */
    changes: string[];
  }

  let { changes }: Props = $props();
</script>

<section class="summary" aria-label="Recorded changes">
  <h3 class="summary-title">Recorded changes to restore</h3>
  {#if changes.length === 0}
    <p class="empty">
      No recorded changes are listed. Reconnect and reconcile the session so
      the recorded changes can load before restoring.
    </p>
  {:else}
    <p class="lede">
      The restore undoes these recorded changes in reverse order, then verifies
      the restored state on the phone.
    </p>
    <ol class="changes">
      {#each changes as change, index (index)}
        <li>{change}</li>
      {/each}
    </ol>
  {/if}
</section>

<style>
  .summary {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
    background: var(--unscroll-card);
    border: 1px solid var(--unscroll-border);
    border-radius: var(--unscroll-radius-small);
    padding: 14px 16px;
  }

  .summary-title {
    margin: 0;
    font-size: 15px;
    font-weight: 700;
  }

  .lede,
  .empty {
    margin: 0;
    max-width: 72ch;
    color: var(--unscroll-muted);
  }

  .changes {
    margin: 0;
    padding-left: 22px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-width: 72ch;
  }
</style>
