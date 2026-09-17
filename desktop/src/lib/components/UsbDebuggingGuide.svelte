<section class="guide" aria-label="After setup: USB debugging">
  <h3 class="guide-title">After setup: turn off USB debugging</h3>
  <ol class="steps">
    <li>On the phone, open <strong>Settings</strong>, then <strong>System</strong>, then <strong>Developer Options</strong>.</li>
    <li>Turn off <strong>USB debugging</strong>.</li>
    <li>Optionally, turn off <strong>Developer Options</strong> at the top of the same screen.</li>
  </ol>
  <p class="warning">
    <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" focusable="false">
      <path d="M8 2 14.5 13.5H1.5Z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" />
      <line x1="8" y1="6.4" x2="8" y2="10" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />
      <circle cx="8" cy="12" r="1" fill="currentColor" />
    </svg>
    <span>
      Leaving USB debugging enabled can conflict with the security expectations
      of banking, government, workplace, or other sensitive apps. Editing,
      maintenance, and restoration later require re-enabling debugging and
      reconnecting the phone.
    </span>
  </p>
</section>

<style>
  .guide {
    background: var(--unscroll-card);
    border: 1px solid var(--unscroll-border);
    border-radius: var(--unscroll-radius-small);
    padding: 14px 16px;
    min-width: 0;
  }

  .guide-title {
    margin: 0 0 8px;
    font-size: 15px;
    font-weight: 700;
  }

  .steps {
    margin: 0;
    padding-left: 22px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .warning {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    margin: 12px 0 0;
    font-weight: 600;
    min-width: 0;
  }

  .warning svg {
    flex: none;
    margin-top: 1px;
    color: var(--unscroll-sage);
  }
</style>
