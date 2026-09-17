<script lang="ts">
  interface Props {
    /** Human text for the current step (never a raw event name). */
    current: string;
    /** Count of verified completed operations, in journal order. */
    verified: number;
    /** Verified rollback steps so far (0 while applying forward). */
    rollbackSteps: number;
    /** Concrete recovery action shown adjacent to a failure, if any. */
    recovery: string | null;
    /** True while a failure state is shown. */
    failed: boolean;
  }

  let { current, verified, rollbackSteps, recovery, failed }: Props = $props();
</script>

<section class="progress" aria-label="Apply progress">
  <h3 class="progress-title">Apply progress</h3>
  <ol class="progress-steps">
    <li class="step step-current">
      <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" focusable="false">
        <circle cx="8" cy="8" r="6.5" fill="none" stroke="currentColor" stroke-width="2" stroke-dasharray="4 3" />
      </svg>
      <span><strong>Current step:</strong> {current}</span>
    </li>
    <li class="step step-verified">
      <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" focusable="false">
        <circle cx="8" cy="8" r="6.5" fill="none" stroke="currentColor" stroke-width="2" />
        <path d="M5.2 8.4 7.2 10.4 11 6" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
      <span><strong>Verified completed steps:</strong> {verified}{rollbackSteps > 0 ? ` (rollback steps verified: ${rollbackSteps})` : ""}</span>
    </li>
  </ol>
  {#if failed && recovery}
    <p class="recovery">
      <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" focusable="false">
        <circle cx="8" cy="8" r="6.5" fill="none" stroke="currentColor" stroke-width="2" />
        <line x1="8" y1="4.6" x2="8" y2="9" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
        <circle cx="8" cy="11.4" r="1.1" fill="currentColor" />
      </svg>
      <span><strong>Recovery action:</strong> {recovery}</span>
    </p>
  {/if}
</section>

<style>
  /* Progress is announced through the shell's single polite live region;
     no nested live roles here, so updates are read exactly once. */
  .progress {
    background: var(--unscroll-card);
    border: 1px solid var(--unscroll-border);
    border-radius: var(--unscroll-radius-small);
    padding: 14px 16px;
    min-width: 0;
  }

  .progress-title {
    margin: 0 0 8px;
    font-size: 15px;
    font-weight: 700;
  }

  .progress-steps {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }

  .step,
  .recovery {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    margin: 0;
    min-width: 0;
  }

  .step svg,
  .recovery svg {
    flex: none;
    margin-top: 1px;
    color: var(--unscroll-sage);
  }

  .recovery {
    margin-top: 10px;
    font-weight: 600;
  }
</style>
