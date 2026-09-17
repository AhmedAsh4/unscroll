<script lang="ts">
  import ProtectionSummary from "../components/ProtectionSummary.svelte";
  import UsbDebuggingGuide from "../components/UsbDebuggingGuide.svelte";

  interface Props {
    /** Optional sideload gaps verified by the backend complete outcome. */
    partialProtection: string[];
  }

  let { partialProtection }: Props = $props();
</script>

<section class="completion" aria-labelledby="completion-heading">
  <h2 id="completion-heading">Setup complete</h2>
  <p class="lede">
    The phone verified Unscroll Launcher as the default and confirmed the
    policy. The desktop did not mark this complete on its own; success is
    shown only after the phone-side verification.
  </p>
  <ProtectionSummary items={partialProtection} />
  <UsbDebuggingGuide />
</section>

<style>
  .completion {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
  }

  .completion h2 {
    margin: 0;
    font-size: 24px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }

  .lede {
    margin: 0;
    max-width: 72ch;
    color: var(--unscroll-muted);
  }
</style>
