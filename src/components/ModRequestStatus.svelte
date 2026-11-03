<script lang="ts">
  import { Clock3, LoaderCircle, Pause } from "@lucide/svelte";
  import { createTranslation } from "../lib/i18n.svelte";
  import { requestStatus, type RequestedState } from "../modRequests";

  let { state }: {
    state: RequestedState | null;
  } = $props();
  const { t } = createTranslation();
  const status = $derived(requestStatus(state));
</script>

{#if status && state}
  <section data-testid="mod-request-status" data-phase={status} class="shrink-0 border-b border-b1 px-3 py-2 text-[11px] text-t2">
    <div role="status" class="flex items-start gap-2">
      <span class="shrink-0 mt-[2px] text-t3">
        {#if status === "preparing"}<LoaderCircle size={12} />
        {:else if status === "blocked"}<Pause size={12} />
        {:else}<Clock3 size={12} />{/if}
      </span>
      <div class="min-w-0 flex-1 break-words">
        <div>{t(`mod_requests.${status}`)}{#if status === "blocked" && state.preparation?.reason}: {t(`mod_requests.${state.preparation.reason}`)}{/if}</div>
        {#if state.activeRevisionId}<div class="mt-[2px] text-[10px] text-t3">{t("mod_requests.current_ready")}</div>{/if}
      </div>
    </div>
  </section>
{/if}
