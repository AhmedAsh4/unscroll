<script lang="ts">
  import type { StepState } from "../state/workspace.ts";

  interface Props {
    steps: StepState[];
  }

  let { steps }: Props = $props();

  function noteFor(step: StepState): string | null {
    if (step.status === "blocked") return "Blocked";
    return null;
  }
</script>

<ol class="rail-list">
  {#each steps as step, index (step.id)}
    {@const current = step.status === "active"}
    {@const done = step.status === "complete"}
    {@const blocked = step.status === "blocked"}
    <li class="rail-item" class:current class:done class:blocked aria-current={current ? "step" : undefined}>
      {#if current}
        <span class="marker" aria-hidden="true">{index + 1}</span>
      {:else if done}
        <span class="marker" aria-hidden="true">
          <svg width="14" height="14" viewBox="0 0 14 14" focusable="false" aria-hidden="true">
            <path d="M2.5 7.5 5.5 10.5 11.5 3.5" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </span>
        <span class="sr-only">{step.label} completed</span>
      {:else if blocked}
        <span class="marker" aria-hidden="true">
          <svg width="14" height="14" viewBox="0 0 14 14" focusable="false" aria-hidden="true">
            <rect x="2.5" y="6" width="9" height="6" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.8" />
            <path d="M4.5 6V4.5a2.5 2.5 0 0 1 5 0V6" fill="none" stroke="currentColor" stroke-width="1.8" />
          </svg>
        </span>
        <span class="sr-only">{step.label} blocked</span>
      {:else}
        <span class="marker marker-idle" aria-hidden="true">{index + 1}</span>
      {/if}
      <span class="step-text">
        <span class="step-label">{step.label}</span>
        {#if noteFor(step)}
          <span class="step-note">{noteFor(step)}</span>
        {/if}
      </span>
    </li>
  {/each}
</ol>

<style>
  .rail-list {
    list-style: none;
    margin: 0;
    padding: 22px 16px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .rail-item {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 9px 10px;
    border-radius: var(--unscroll-radius-small);
    color: rgba(255, 255, 255, 0.72);
    min-width: 0;
  }

  .rail-item.current {
    background: rgba(255, 255, 255, 0.12);
    color: #ffffff;
    font-weight: 700;
  }

  .rail-item.done {
    color: #ffffff;
  }

  .rail-item.blocked {
    color: rgba(255, 255, 255, 0.72);
  }

  .marker {
    flex: none;
    width: 26px;
    height: 26px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    font-size: 13px;
    font-weight: 700;
    background: rgba(255, 255, 255, 0.14);
    color: #ffffff;
  }

  .marker-idle {
    background: transparent;
    border: 1.5px solid rgba(255, 255, 255, 0.4);
    color: rgba(255, 255, 255, 0.72);
  }

  .step-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.3;
  }

  .step-label {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .step-note {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgba(255, 255, 255, 0.72);
  }
</style>
