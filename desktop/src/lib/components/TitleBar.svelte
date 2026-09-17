<script lang="ts">
  import ConnectionStatus from "./ConnectionStatus.svelte";

  interface Props {
    statusState: "ok" | "busy" | "error" | "idle";
    statusText: string;
  }

  let { statusState, statusText }: Props = $props();
</script>

<div class="title-bar">
  <span class="brand" aria-hidden="true">
    <svg width="26" height="26" viewBox="0 0 26 26" focusable="false">
      <circle cx="13" cy="13" r="12" fill="none" stroke="#245c47" stroke-width="2.5" />
      <path d="M8 14.5 12 18.5 18.5 8.5" fill="none" stroke="#245c47" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" />
    </svg>
  </span>
  <h1 class="app-name">Unscroll</h1>
  <span class="spacer"></span>
  <ConnectionStatus state={statusState} text={statusText} />
</div>

<style>
  .title-bar {
    height: var(--unscroll-bar-height);
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 20px;
    min-width: 0;
  }

  .brand {
    display: inline-flex;
    flex: none;
  }

  .app-name {
    margin: 0;
    font-size: 17px;
    font-weight: 700;
    letter-spacing: 0.01em;
    white-space: nowrap;
  }

  .spacer {
    flex: 1 1 auto;
    min-width: 0;
  }
</style>
