<script lang="ts">
  import type { PillState } from "../state/workspace.ts";

  interface Props {
    state: PillState;
  }

  let { state }: Props = $props();

  const label = $derived(
    state === "kept" ? "Kept" : state === "blocked" ? "Blocked" : state === "protected" ? "Protected" : state === "store" ? "Store" : "Unsupported",
  );
</script>

<span class="pill pill-{state}">
  <span class="pill-icon" aria-hidden="true">
    {#if state === "kept"}
      <svg width="12" height="12" viewBox="0 0 12 12" focusable="false">
        <path d="M2 6.5 4.8 9 10 3.2" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
    {:else if state === "blocked"}
      <svg width="12" height="12" viewBox="0 0 12 12" focusable="false">
        <circle cx="6" cy="6" r="4.4" fill="none" stroke="currentColor" stroke-width="1.8" />
        <line x1="3.2" y1="3.2" x2="8.8" y2="8.8" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />
      </svg>
    {:else if state === "protected"}
      <svg width="12" height="12" viewBox="0 0 12 12" focusable="false">
        <rect x="2.5" y="5.2" width="7" height="4.8" rx="1" fill="none" stroke="currentColor" stroke-width="1.6" />
        <path d="M4 5.2V3.8a2 2 0 0 1 4 0v1.4" fill="none" stroke="currentColor" stroke-width="1.6" />
      </svg>
    {:else if state === "store"}
      <svg width="12" height="12" viewBox="0 0 12 12" focusable="false">
        <path d="M2 4.2 3.2 10h5.6L10 4.2Z" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" />
        <path d="M2 4.2 6 2l4 2.2" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" />
      </svg>
    {:else}
      <svg width="12" height="12" viewBox="0 0 12 12" focusable="false">
        <path d="M6 1.8 11 10H1Z" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" />
        <line x1="6" y1="4.6" x2="6" y2="7.2" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
      </svg>
    {/if}
  </span>
  {label}
</span>

<style>
  /* Icon + text always; state is never carried by color alone. */
  .pill {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.02em;
    border: 1px solid var(--unscroll-border);
    border-radius: 999px;
    padding: 2px 9px 2px 7px;
    background: var(--unscroll-card);
    color: var(--unscroll-text);
    white-space: nowrap;
  }

  .pill-icon {
    display: inline-flex;
  }

  .pill-kept .pill-icon {
    color: var(--unscroll-sage);
  }

  .pill-blocked .pill-icon {
    color: #8c2f1b;
  }

  .pill-protected .pill-icon {
    color: var(--unscroll-sage);
  }

  .pill-store .pill-icon {
    color: #7a5410;
  }

  .pill-unsupported .pill-icon {
    color: #7a5410;
  }
</style>
