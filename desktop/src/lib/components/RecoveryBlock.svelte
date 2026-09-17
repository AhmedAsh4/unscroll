<script lang="ts">
  import { routeForSession } from "../state/workspace.ts";
  import type { SessionKindName } from "../api/types.ts";

  interface Props {
    /** The blocked session class (histories disagree, or copies are missing). */
    kind: SessionKindName;
    /** Server-provided guidance for this session. */
    guidance: string;
    /** Opens the diagnostics view, the only safe next step from here. */
    onExportDiagnostics: () => void;
  }

  let { kind, guidance, onExportDiagnostics }: Props = $props();

  const route = $derived(routeForSession(kind));
</script>

<section class="blocked" aria-labelledby="recovery-blocked-heading">
  <h3 id="recovery-blocked-heading">{route.title}</h3>
  <p class="lede">{route.body}</p>
  <p class="guidance">{guidance}</p>
  <p class="safe">
    No automatic change is safe from here. The only safe next step is to
    review a diagnostic report and decide manually.
  </p>
  <div class="actions">
    <button type="button" class="u-button-primary" onclick={onExportDiagnostics}>
      Review diagnostics
    </button>
  </div>
</section>

<style>
  .blocked {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
    background: var(--unscroll-card);
    border: 1px solid var(--unscroll-border);
    border-left-width: 4px;
    border-left-color: #8c2f1b;
    border-radius: var(--unscroll-radius-small);
    padding: 14px 16px;
  }

  .blocked h3 {
    margin: 0;
    font-size: 16px;
    font-weight: 700;
  }

  .lede {
    margin: 0;
    max-width: 72ch;
    color: var(--unscroll-muted);
  }

  .guidance,
  .safe {
    margin: 0;
    max-width: 72ch;
  }

  .safe {
    font-weight: 600;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }
</style>
