<script lang="ts">
  import AppShell from "./lib/components/AppShell.svelte";
  import ConnectScreen from "./lib/screens/ConnectScreen.svelte";
  import { STEPS, type ShellUpdate, type StepState } from "./lib/state/workspace.ts";

  let steps: StepState[] = $state(STEPS.map((step, index) => ({ ...step, status: index === 0 ? "active" as const : "pending" as const })));
  let announcement = $state("");
  let busy = $state(false);
  let statusState: ShellUpdate["statusState"] = $state("idle");
  let statusText = $state("No phone connected");

  function handleState(update: ShellUpdate): void {
    steps = update.steps;
    announcement = update.announcement;
    busy = update.busy;
    statusState = update.statusState;
    statusText = update.statusText;
  }
</script>

<AppShell {steps} {announcement} {busy} {statusState} {statusText}>
  <ConnectScreen onState={handleState} />
</AppShell>
