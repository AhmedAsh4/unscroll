<script lang="ts">
  import type { AppEntryDto } from "../api/types.ts";
  import type { PillState } from "../state/workspace.ts";
  import { fallbackInitial } from "../state/workspace.ts";
  import StatePill from "./StatePill.svelte";
  import SelectionSwitch from "./SelectionSwitch.svelte";

  interface Props {
    entry: AppEntryDto;
    kept: boolean;
    locked: boolean;
    pill: PillState;
    /** Resolved local icon URL, or null for the neutral local fallback. */
    iconSrc: string | null;
    onToggle: (packageId: string) => void;
  }

  let { entry, kept, locked, pill, iconSrc, onToggle }: Props = $props();

  let imgFailed = $state(false);
  const showImg = $derived(iconSrc !== null && !imgFailed);
</script>

<li class="app-row" role="listitem">
  <span class="app-icon" aria-hidden="true">
    {#if showImg}
      <img
        class="app-icon-img"
        src={iconSrc}
        alt=""
        width="33"
        height="33"
        onerror={() => {
          imgFailed = true;
        }}
      />
    {:else}
      <span class="app-icon-fallback" aria-hidden="true">{fallbackInitial(entry.label)}</span>
    {/if}
  </span>
  <span class="app-text">
    <span class="app-label">{entry.label}</span>
    <span class="app-package">{entry.packageId}</span>
  </span>
  <StatePill state={pill} />
  <SelectionSwitch
    label={entry.label}
    packageId={entry.packageId}
    checked={kept}
    locked={locked}
    protectedReason={entry.protectedReason}
    {onToggle}
  />
</li>

<style>
  /* Compact 59px rows from the approved mockup treatment. */
  .app-row {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 59px;
    padding: 8px 12px;
    background: var(--unscroll-card);
    border: 1px solid var(--unscroll-border);
    border-radius: var(--unscroll-radius-small);
    min-width: 0;
  }

  .app-icon {
    flex: none;
    display: inline-flex;
  }

  .app-icon-img,
  .app-icon-fallback {
    width: 33px;
    height: 33px;
    border-radius: 8px;
  }

  .app-icon-img {
    object-fit: cover;
    background: var(--unscroll-surface);
  }

  /* Neutral local fallback: initial letter on a warm surface. */
  .app-icon-fallback {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 16px;
    font-weight: 700;
    color: var(--unscroll-sage);
    background: var(--unscroll-surface);
    border: 1px solid var(--unscroll-border);
  }

  .app-text {
    flex: 1 1 auto;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .app-label {
    font-weight: 600;
    overflow-wrap: break-word;
  }

  .app-package {
    font-size: 12px;
    color: var(--unscroll-muted);
    overflow-wrap: break-word;
  }
</style>
