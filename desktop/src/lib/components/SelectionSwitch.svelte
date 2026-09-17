<script lang="ts">
  interface Props {
    label: string;
    packageId: string;
    /** True = kept/available; false = blocked. */
    checked: boolean;
    /** Protected entries stay locked on kept with their reason exposed. */
    locked: boolean;
    protectedReason: string | null;
    onToggle: (packageId: string) => void;
  }

  let { label, packageId, checked, locked, protectedReason, onToggle }: Props = $props();

  const stateText = $derived(locked ? "protected" : checked ? "kept" : "blocked");
  const accessibleName = $derived(
    locked && protectedReason ? `${label}, ${packageId}, ${stateText}, ${protectedReason}` : `${label}, ${packageId}, ${stateText}`,
  );
</script>

<span class="switch-wrap">
  <button
    type="button"
    role="switch"
    aria-checked={checked}
    aria-label={accessibleName}
    disabled={locked}
    class="switch"
    class:on={checked}
    onclick={() => {
      if (!locked) onToggle(packageId);
    }}
  >
    <span class="knob" aria-hidden="true">
      {#if locked}
        <svg width="11" height="11" viewBox="0 0 12 12" focusable="false">
          <rect x="2.5" y="5.2" width="7" height="4.8" rx="1" fill="none" stroke="currentColor" stroke-width="1.8" />
          <path d="M4 5.2V3.8a2 2 0 0 1 4 0v1.4" fill="none" stroke="currentColor" stroke-width="1.8" />
        </svg>
      {/if}
    </span>
  </button>
  {#if locked && protectedReason}
    <span class="lock-reason">{protectedReason}</span>
  {/if}
</span>

<style>
  .switch-wrap {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  /* Native button: keyboard operable with visible global focus. */
  .switch {
    flex: none;
    width: 40px;
    height: 22px;
    border-radius: 999px;
    border: 1px solid var(--unscroll-border);
    background: var(--unscroll-surface);
    padding: 0;
    position: relative;
  }

  .switch.on {
    background: var(--unscroll-sage);
    border-color: var(--unscroll-sage);
  }

  .switch:disabled {
    opacity: 0.75;
  }

  .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--unscroll-card);
    border: 1px solid var(--unscroll-border);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--unscroll-sage);
  }

  .switch.on .knob {
    left: 20px;
  }

  .lock-reason {
    font-size: 12px;
    color: var(--unscroll-muted);
    overflow-wrap: break-word;
    min-width: 0;
  }

  @media (prefers-reduced-motion: reduce) {
    .knob {
      transition: none;
    }
  }
</style>
