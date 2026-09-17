<script lang="ts">
  interface Props {
    /** Device model, e.g. "Pixel 8". Identity values are never shown. */
    model: string | null;
    manufacturer: string | null;
    /** Human Android label, e.g. "Android 14 (API 34)". */
    platformLabel: string | null;
    appCount: number | null;
  }

  let { model, manufacturer, platformLabel, appCount }: Props = $props();
</script>

<section class="device-card" aria-label="Connected phone">
  <span class="device-icon" aria-hidden="true">
    <svg width="30" height="30" viewBox="0 0 30 30" focusable="false">
      <rect x="9" y="3" width="12" height="24" rx="2.5" fill="none" stroke="#245c47" stroke-width="2" />
      <line x1="13" y1="24" x2="17" y2="24" stroke="#245c47" stroke-width="2" stroke-linecap="round" />
    </svg>
  </span>
  <div class="device-text">
    <h3 class="device-name">{model ?? "Connected phone"}</h3>
    {#if manufacturer || platformLabel}
      <p class="device-meta">
        {#if manufacturer}{manufacturer}{/if}{#if manufacturer && platformLabel} · {/if}{#if platformLabel}{platformLabel}{/if}
      </p>
    {/if}
    {#if appCount !== null}
      <p class="device-count">{appCount} {appCount === 1 ? "app" : "apps"} found on the phone</p>
    {/if}
  </div>
</section>

<style>
  .device-card {
    display: flex;
    align-items: flex-start;
    gap: 14px;
    background: var(--unscroll-card);
    border: 1px solid var(--unscroll-border);
    border-radius: var(--unscroll-radius);
    padding: 16px 18px;
    min-width: 0;
  }

  .device-icon {
    flex: none;
    display: inline-flex;
    margin-top: 2px;
  }

  .device-text {
    min-width: 0;
  }

  .device-name {
    margin: 0 0 2px;
    font-size: 16px;
    font-weight: 700;
    overflow-wrap: break-word;
  }

  .device-meta,
  .device-count {
    margin: 0;
    font-size: 14px;
    color: var(--unscroll-muted);
  }

  .device-count {
    margin-top: 4px;
    color: var(--unscroll-text);
    font-weight: 600;
  }
</style>
