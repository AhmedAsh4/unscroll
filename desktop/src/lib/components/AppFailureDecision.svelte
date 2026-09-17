<script lang="ts">
  import { onMount } from "svelte";

  interface FailedApp {
    packageId: string;
    label: string;
  }

  interface Props {
    /** Every affected app, named by label and packageId. */
    apps: FailedApp[];
    /** True while an answer is being sent (both answers disable). */
    busy: boolean;
    onContinue: () => void;
    onRollback: () => void;
  }

  let { apps, busy, onContinue, onRollback }: Props = $props();
  let heading: HTMLHeadingElement | null = $state(null);

  onMount(() => {
    heading?.focus();
  });
</script>

<section class="decision" aria-labelledby="apply-decision-heading">
  <h2 id="apply-decision-heading" tabindex="-1" bind:this={heading}>One app needs your decision</h2>
  <p class="lede">
    This app could not be fully protected, so it will stay available if you
    continue. Decide before the home screen changes: nothing there has changed yet.
  </p>
  <ul class="affected">
    {#each apps as app (app.packageId)}
      <li>
        <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" focusable="false">
          <path d="M8 2 14.5 13.5H1.5Z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" />
          <line x1="8" y1="6.4" x2="8" y2="10" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />
          <circle cx="8" cy="12" r="1" fill="currentColor" />
        </svg>
        <span><strong>{app.label}</strong> ({app.packageId})</span>
      </li>
    {/each}
  </ul>
  <div class="actions">
    <button type="button" class="u-button-primary" disabled={busy} onclick={onContinue}>
      Continue with available
    </button>
    <button type="button" class="u-button-secondary" disabled={busy} onclick={onRollback}>
      Roll back
    </button>
  </div>
</section>

<style>
  .decision {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
  }

  .decision h2 {
    margin: 0;
    font-size: 20px;
    font-weight: 700;
  }

  .decision h2:focus {
    outline-offset: 3px;
  }

  .lede {
    margin: 0;
    max-width: 72ch;
    color: var(--unscroll-muted);
  }

  .affected {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .affected li {
    display: flex;
    align-items: center;
    gap: 10px;
    background: var(--unscroll-card);
    border: 1px solid var(--unscroll-border);
    border-radius: var(--unscroll-radius-small);
    padding: 10px 12px;
    min-width: 0;
  }

  .affected svg {
    flex: none;
    color: var(--unscroll-sage);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }
</style>
