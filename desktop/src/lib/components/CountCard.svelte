<script lang="ts">
  import { countText } from "../state/workspace.ts";

  interface Props {
    kept: number;
    blocked: number;
    total: number;
  }

  let { kept, blocked, total }: Props = $props();
</script>

<div class="count-card">
  <p class="count-text">{countText(kept, blocked, total)}</p>
</div>

<style>
  .count-card {
    background: var(--unscroll-card);
    border: 1px solid var(--unscroll-border);
    border-radius: var(--unscroll-radius-small);
    padding: 10px 14px;
    min-width: 0;
  }

  .count-text {
    margin: 0;
    font-weight: 700;
    font-size: 14px;
  }
</style>
