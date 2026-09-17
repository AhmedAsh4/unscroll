<script lang="ts">
  import { MAINTENANCE_CONFIRMATION } from "../api/types.ts";
  import TypedConfirmation from "./TypedConfirmation.svelte";

  interface Props {
    /** True while the open request is running (confirmation and button disable). */
    busy: boolean;
    /** Fires only with the exact typed acknowledgment; mismatches never fire. */
    onOpen: (confirmation: string) => void;
  }

  let { busy, onOpen }: Props = $props();
</script>

<section class="warning" aria-labelledby="maintenance-warning-heading">
  <h3 id="maintenance-warning-heading">Opening store maintenance has a cost</h3>
  <p class="lede">
    Maintenance temporarily restores the recorded store and install-source
    state so updates can run on the phone. Read this before typing the
    acknowledgment.
  </p>
  <ul class="points">
    <li>While the window is open, new installs are possible on the phone.</li>
    <li>Ordinary exit stays guarded until a verified close re-applies the store restrictions.</li>
    <li>Closing re-scans the phone: new apps outside the allowlist are recorded and restricted again.</li>
  </ul>
  <TypedConfirmation
    expected={MAINTENANCE_CONFIRMATION}
    {busy}
    confirmLabel="Open store maintenance"
    hint="Typing OPEN STORE MAINTENANCE acknowledges that new installs are possible until the verified close."
    onConfirm={onOpen}
  />
</section>

<style>
  .warning {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
    background: var(--unscroll-card);
    border: 1px solid var(--unscroll-border);
    border-left-width: 4px;
    border-left-color: #9a6a14;
    border-radius: var(--unscroll-radius-small);
    padding: 14px 16px;
  }

  .warning h3 {
    margin: 0;
    font-size: 16px;
    font-weight: 700;
  }

  .lede {
    margin: 0;
    max-width: 72ch;
    color: var(--unscroll-muted);
  }

  .points {
    margin: 0;
    padding-left: 22px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-width: 72ch;
  }
</style>
