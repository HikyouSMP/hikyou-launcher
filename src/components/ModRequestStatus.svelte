<script lang="ts">
  import { Clock3, LoaderCircle, Pause, Trash2 } from "@lucide/svelte";
  import { createTranslation } from "../lib/i18n.svelte";
  import { pendingRequirements, requestedKey, requestStatus, requirementName, type IdentifiedModFile, type RequestedState } from "../modRequests";

  let { state, files, busy = false, onRemove }: {
    state: RequestedState | null;
    files: IdentifiedModFile[];
    busy?: boolean;
    onRemove: (key: string) => void;
  } = $props();
  const { t } = createTranslation();
  const status = $derived(requestStatus(state));
  const pending = $derived(pendingRequirements(state, files));
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
    {#if pending.length}
      <ul class="sb max-h-[112px] overflow-y-auto mt-1 list-none p-0 m-0">
        {#each pending as requirement (requestedKey(requirement) ?? requirementName(requirement))}
          {@const key = requestedKey(requirement)}
          {@const name = requirementName(requirement)}
          <li data-requested-key={key} class="flex items-center gap-2 min-h-7 pl-5">
            <span class="min-w-0 flex-1 truncate" title={name}>{name}</span>
            <span class="shrink-0 text-[10px] text-t3">{t(!requirement.enabled ? "mod_requests.requested_disabled" : requirement.projectId ? "mod_requests.pending_root" : "mod_requests.requested")}</span>
            {#if key}
              <button class="icon-btn shrink-0" disabled={busy} title={t("mod_requests.remove_request", { name })} aria-label={t("mod_requests.remove_request", { name })} onclick={() => onRemove(key)}><Trash2 size={12} /></button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </section>
{/if}
