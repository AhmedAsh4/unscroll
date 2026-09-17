<script lang="ts">
  import { onMount } from "svelte";

  interface Props {
    /** The exact phrase the user must type (compared verbatim, never trimmed). */
    expected: string;
    /** True while the confirmed action is running (input and button disable). */
    busy: boolean;
    /** Label for the confirm button. */
    confirmLabel: string;
    /** Short explainer shown above the input. */
    hint: string;
    /** Fires only when the typed value matches exactly; mismatches never fire. */
    onConfirm: (value: string) => void;
  }

  let { expected, busy, confirmLabel, hint, onConfirm }: Props = $props();

  let value = $state("");
  let input: HTMLInputElement | null = $state(null);
  const fieldId = $props.id();
  const inputId = `typed-confirmation-${fieldId}`;
  const reasonId = `typed-confirmation-reason-${fieldId}`;

  const mismatch = $derived(value !== expected);

  onMount(() => {
    input?.focus();
  });

  function confirmNow(): void {
    if (busy || value !== expected) return;
    onConfirm(value);
  }
</script>

<div class="typed">
  <p class="hint">{hint}</p>
  <label class="field-label" for={inputId}>
    Type <strong>{expected}</strong> to confirm
  </label>
  <input
    id={inputId}
    class="field-input"
    type="text"
    autocomplete="off"
    spellcheck="false"
    bind:value
    bind:this={input}
    disabled={busy}
    aria-describedby={reasonId}
  />
  {#if value.length > 0 && mismatch}
    <p class="reason" id={reasonId}>
      The phrase does not match yet. Nothing starts until it matches exactly.
    </p>
  {:else}
    <p class="reason-ok" id={reasonId}>Matching is exact, including capitals and spaces.</p>
  {/if}
  <div class="actions">
    <button type="button" class="u-button-primary" disabled={busy || mismatch} onclick={confirmNow}>
      {confirmLabel}
    </button>
  </div>
</div>

<style>
  .typed {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }

  .hint {
    margin: 0;
    max-width: 72ch;
    color: var(--unscroll-muted);
  }

  .field-label {
    font-weight: 600;
  }

  .field-input {
    font: inherit;
    color: var(--unscroll-text);
    background: var(--unscroll-card);
    border: 1px solid var(--unscroll-border);
    border-radius: var(--unscroll-radius-small);
    padding: 10px 12px;
    max-width: 40ch;
  }

  .reason {
    margin: 0;
    font-weight: 600;
  }

  .reason-ok {
    margin: 0;
    color: var(--unscroll-muted);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }
</style>
