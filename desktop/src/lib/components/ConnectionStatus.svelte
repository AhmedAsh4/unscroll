<script lang="ts">
  interface Props {
    state: "ok" | "busy" | "error" | "idle";
    text: string;
  }

  let { state, text }: Props = $props();
</script>

<span class="conn conn-{state}">
  <span class="conn-icon" aria-hidden="true">
    {#if state === "ok"}
      <svg width="14" height="14" viewBox="0 0 14 14" focusable="false">
        <circle cx="7" cy="7" r="5.5" fill="none" stroke="currentColor" stroke-width="2" />
        <path d="M4.5 7.2 6.3 9 9.7 5.2" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
    {:else if state === "busy"}
      <svg width="14" height="14" viewBox="0 0 14 14" focusable="false">
        <circle cx="7" cy="7" r="5.5" fill="none" stroke="currentColor" stroke-width="2" stroke-dasharray="3 2.4" />
      </svg>
    {:else if state === "error"}
      <svg width="14" height="14" viewBox="0 0 14 14" focusable="false">
        <circle cx="7" cy="7" r="5.5" fill="none" stroke="currentColor" stroke-width="2" />
        <line x1="7" y1="4" x2="7" y2="8" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
        <circle cx="7" cy="10.2" r="1.1" fill="currentColor" />
      </svg>
    {:else}
      <svg width="14" height="14" viewBox="0 0 14 14" focusable="false">
        <circle cx="7" cy="7" r="5.5" fill="none" stroke="currentColor" stroke-width="2" />
      </svg>
    {/if}
  </span>
  <span class="conn-text">{text}</span>
</span>

<style>
  /* Status is always icon + text, never color alone. */
  .conn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 13.5px;
    font-weight: 600;
    white-space: nowrap;
    min-width: 0;
  }

  .conn-icon {
    display: inline-flex;
    flex: none;
  }

  .conn-text {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .conn-ok {
    color: var(--unscroll-sage);
  }

  .conn-busy {
    color: var(--unscroll-text);
  }

  .conn-error {
    color: #8c2f1b;
  }

  .conn-idle {
    color: var(--unscroll-muted);
  }
</style>
