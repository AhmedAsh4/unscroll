<script lang="ts">
  interface Props {
    tone: "info" | "warning" | "error";
    title: string;
    message: string;
    /** Recovery action rendered adjacent to the message. */
    actionLabel?: string | null;
    onAction?: (() => void) | null;
    primary?: boolean;
    /** Renders the adjacent action visibly disabled (passive-wait states). */
    actionDisabled?: boolean;
    /** Optional heading-id override; defaults to a per-instance unique id. */
    id?: string;
  }

  let { tone, title, message, actionLabel = null, onAction = null, primary = false, actionDisabled = false, id: idProp }: Props = $props();
  const uid = $props.id();
  const headingId = $derived(idProp ?? `notice-${uid}`);
</script>

<div class="notice notice-{tone}" aria-labelledby={headingId}>
  <span class="notice-icon" aria-hidden="true">
    {#if tone === "error"}
      <svg width="18" height="18" viewBox="0 0 18 18" focusable="false">
        <circle cx="9" cy="9" r="7.5" fill="none" stroke="currentColor" stroke-width="2" />
        <line x1="9" y1="5.2" x2="9" y2="10" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
        <circle cx="9" cy="12.8" r="1.2" fill="currentColor" />
      </svg>
    {:else if tone === "warning"}
      <svg width="18" height="18" viewBox="0 0 18 18" focusable="false">
        <path d="M9 2.5 16.5 15.5H1.5Z" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" />
        <line x1="9" y1="7" x2="9" y2="11" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
        <circle cx="9" cy="13.2" r="1.1" fill="currentColor" />
      </svg>
    {:else}
      <svg width="18" height="18" viewBox="0 0 18 18" focusable="false">
        <circle cx="9" cy="9" r="7.5" fill="none" stroke="currentColor" stroke-width="2" />
        <line x1="9" y1="8.2" x2="9" y2="12.6" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
        <circle cx="9" cy="5.6" r="1.2" fill="currentColor" />
      </svg>
    {/if}
  </span>
  <div class="notice-body">
    <h3 class="notice-title" id={headingId}>{title}</h3>
    <p class="notice-message">{message}</p>
    {#if actionLabel}
      <button
        type="button"
        class={primary ? "u-button-primary" : "u-button-secondary"}
        disabled={actionDisabled}
        onclick={() => {
          if (!actionDisabled) onAction?.();
        }}
      >
        {actionLabel}
      </button>
    {/if}
  </div>
</div>

<style>
  /* Notices are announced through the shell's single polite live region;
     no nested live roles here, so changes are read exactly once. */
  .notice {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    background: var(--unscroll-card);
    border: 1px solid var(--unscroll-border);
    border-left-width: 4px;
    border-radius: var(--unscroll-radius-small);
    padding: 14px 16px;
    min-width: 0;
  }

  .notice-info {
    border-left-color: var(--unscroll-sage);
  }

  .notice-warning {
    border-left-color: #9a6a14;
  }

  .notice-error {
    border-left-color: #8c2f1b;
  }

  .notice-icon {
    flex: none;
    display: inline-flex;
    margin-top: 1px;
  }

  .notice-info .notice-icon {
    color: var(--unscroll-sage);
  }

  .notice-warning .notice-icon {
    color: #7a5410;
  }

  .notice-error .notice-icon {
    color: #8c2f1b;
  }

  .notice-body {
    min-width: 0;
  }

  .notice-title {
    margin: 0 0 2px;
    font-size: 15px;
    font-weight: 700;
  }

  .notice-message {
    margin: 0 0 12px;
    color: var(--unscroll-text);
  }

  .notice-body button:last-child {
    margin-bottom: 2px;
  }
</style>
