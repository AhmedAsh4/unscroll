<script lang="ts">
  import type { DiagnosticPreviewDto } from "../api/types.ts";

  interface Props {
    /** Redacted preview DTO: identity values arrive redacted from the backend. */
    preview: DiagnosticPreviewDto;
  }

  let { preview }: Props = $props();
</script>

<section class="preview" aria-label="Diagnostic preview">
  <h3 class="preview-title">Diagnostic preview</h3>
  <p class="lede">
    Review everything below before choosing an export destination. Identity
    values arrive redacted; the export carries this redacted copy only.
  </p>
  <dl class="facts">
    <div class="fact">
      <dt>Device model</dt>
      <dd>{preview.deviceModel}</dd>
    </div>
    <div class="fact">
      <dt>Redacted fingerprint</dt>
      <dd>{preview.fingerprintRedacted}</dd>
    </div>
    <div class="fact">
      <dt>Allowlist entries</dt>
      <dd>{preview.allowlistCount}</dd>
    </div>
    <div class="fact">
      <dt>Baseline packages</dt>
      <dd>{preview.initialPackageCount}</dd>
    </div>
  </dl>
  <h4 class="list-title">Recorded operations ({preview.operations.length})</h4>
  {#if preview.operations.length === 0}
    <p class="empty">No operations recorded.</p>
  {:else}
    <ul class="items">
      {#each preview.operations as operation, index (index)}
        <li>{operation}</li>
      {/each}
    </ul>
  {/if}
  <h4 class="list-title">Errors ({preview.errors.length})</h4>
  {#if preview.errors.length === 0}
    <p class="empty">No errors recorded.</p>
  {:else}
    <ul class="items">
      {#each preview.errors as error, index (index)}
        <li>{error}</li>
      {/each}
    </ul>
  {/if}
  <h4 class="list-title">Warnings ({preview.warnings.length})</h4>
  {#if preview.warnings.length === 0}
    <p class="empty">No warnings recorded.</p>
  {:else}
    <ul class="items">
      {#each preview.warnings as warning, index (index)}
        <li>{warning}</li>
      {/each}
    </ul>
  {/if}
  <h4 class="list-title">Redacted envelope</h4>
  <pre class="envelope">{preview.redactedEnvelope}</pre>
</section>

<style>
  .preview {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
    background: var(--unscroll-card);
    border: 1px solid var(--unscroll-border);
    border-radius: var(--unscroll-radius-small);
    padding: 14px 16px;
  }

  .preview-title {
    margin: 0;
    font-size: 15px;
    font-weight: 700;
  }

  .lede,
  .empty {
    margin: 0;
    max-width: 72ch;
    color: var(--unscroll-muted);
  }

  .facts {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .fact {
    display: flex;
    gap: 10px;
    min-width: 0;
  }

  .fact dt {
    font-weight: 700;
    min-width: 22ch;
  }

  .fact dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .list-title {
    margin: 6px 0 0;
    font-size: 14px;
    font-weight: 700;
  }

  .items {
    margin: 0;
    padding-left: 22px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-width: 72ch;
  }

  .envelope {
    margin: 0;
    font-size: 13px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    background: var(--unscroll-surface);
    border: 1px solid var(--unscroll-border);
    border-radius: var(--unscroll-radius-small);
    padding: 10px 12px;
    max-height: 240px;
    overflow: auto;
  }
</style>
