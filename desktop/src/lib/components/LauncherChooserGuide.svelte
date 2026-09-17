<script lang="ts">
  import { onMount } from "svelte";

  interface Props {
    /** True while an answer is being sent (both answers disable). */
    busy: boolean;
    onConfirmed: () => void;
    onCancelled: () => void;
  }

  let { busy, onConfirmed, onCancelled }: Props = $props();
  let heading: HTMLHeadingElement | null = $state(null);

  onMount(() => {
    heading?.focus();
  });
</script>

<section class="chooser" aria-labelledby="chooser-guide-heading">
  <h2 id="chooser-guide-heading" tabindex="-1" bind:this={heading}>Choose Unscroll Launcher on the phone</h2>
  <p class="lede">
    The phone ignored the automatic default-launcher request, so Android must
    hear the choice from you. Nothing is finished until the phone verifies the
    new default.
  </p>
  <ol class="steps">
    <li>On the phone, wait for the Android launcher chooser to appear (a “Select a Home app” or “Choose default launcher” sheet).</li>
    <li>Tap <strong>Unscroll Launcher</strong> in the chooser list.</li>
    <li>Tap <strong>Always</strong> (or <strong>Set as default</strong>) to confirm the choice.</li>
    <li>Return here and choose <strong>I chose Unscroll Launcher</strong> so the desktop can verify the new default.</li>
  </ol>
  <div class="actions">
    <button type="button" class="u-button-primary" disabled={busy} onclick={onConfirmed}>
      I chose Unscroll Launcher
    </button>
    <button type="button" class="u-button-secondary" disabled={busy} onclick={onCancelled}>
      Cancel
    </button>
  </div>
</section>

<style>
  .chooser {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
  }

  .chooser h2 {
    margin: 0;
    font-size: 20px;
    font-weight: 700;
  }

  .chooser h2:focus {
    outline-offset: 3px;
  }

  .lede {
    margin: 0;
    max-width: 72ch;
    color: var(--unscroll-muted);
  }

  .steps {
    margin: 0;
    padding-left: 22px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-width: 72ch;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }
</style>
