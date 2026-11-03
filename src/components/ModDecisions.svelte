<script lang="ts">
  import { createTranslation } from "../lib/i18n.svelte";
  import { useModDecisions } from "../hooks/useModDecisions.svelte";
  import { decisionActions, visibleDecisions } from "../modLifecycle";
  import ModFailureDetails from "./ModFailureDetails.svelte";

  let { profileId = null, panel = false, profileNames = {} }: {
    profileId?: string | null;
    panel?: boolean;
    profileNames?: Record<string, string>;
  } = $props();
  const { t } = createTranslation();
  const model = useModDecisions(() => profileId);
  const visible = $derived(visibleDecisions(model.decisions, panel));
  let confirming = $state<{ token: string; action: "accept" | "revoke" } | null>(null);
</script>

{#if model.loadFailed || model.answerFailed || model.eventsFailed || visible.length}
  <section data-mod-decisions class="shrink-0 border-b border-b1 text-[11px] text-t2 max-h-48 overflow-y-auto sb">
    {#if model.loadFailed || model.answerFailed || model.eventsFailed}
      <div role="alert" class="px-3 py-2 flex items-start gap-2">
        <span class="min-w-0 flex-1 break-words">{t(model.loadFailed ? "mod_decisions.load_failed" : model.eventsFailed ? "mod_lifecycle.events_failed" : "mod_decisions.answer_failed")}</span>
        <button class="modal-btn shrink-0" disabled={model.busy} onclick={() => model.retry()}>{t("common.retry")}</button>
      </div>
    {/if}
    <ModFailureDetails failures={model.failures} />
    {#each visible as decision (`${decision.profileId}:${decision.id}`)}
      {@const token = JSON.stringify([decision.profileId, decision.id, decision.expectedRevision, decision.phase])}
      {@const mods = decision.omitted.map((mod) => mod.name).join(", ")}
      <div data-mod-decision={decision.id} data-phase={decision.phase} class="px-3 py-2">
        <div class="text-[10px] text-t3 break-words">{#if !panel}{profileNames[decision.profileId] ?? t("mod_decisions.profile")} · {/if}{decision.target}</div>
        <p class="m-0 mt-1 break-words">{t(`mod_decisions.${decision.phase}`, { mods })}</p>
        {#if confirming?.token === token}
          <p class="m-0 mt-2 break-words text-t2">{t("mod_decisions.composition_warning")}</p>
        {/if}
        <div class="flex flex-wrap gap-2 mt-2">
          {#each decisionActions(decision.phase) as action (action)}
            {@const isConfirming = confirming?.token === token && confirming.action === action}
            <button class="modal-btn decision-action max-w-full text-left" disabled={model.busy || model.loadFailed} onclick={async () => {
              const answeredToken = token;
              if (action === "defer" || isConfirming) {
                await model.answer(decision, action);
                if (confirming?.token === answeredToken) confirming = null;
              } else confirming = { token, action };
            }}>{t(isConfirming ? `mod_decisions.confirm_${action}` : `mod_decisions.${action}`, { mods })}</button>
          {/each}
          {#if confirming?.token === token}
            <button class="modal-btn" disabled={model.busy} onclick={() => confirming = null}>{t("common.cancel")}</button>
          {/if}
        </div>
      </div>
    {/each}
  </section>
{/if}

<style>
  .decision-action { height: auto; min-height: 32px; padding-top: 6px; padding-bottom: 6px; white-space: normal; overflow-wrap: anywhere; }
</style>
