<script lang="ts">
  import { createTranslation } from "../lib/i18n.svelte";
  import { css } from "../lib/dom";
  import { C } from "../theme";
  import type { AutoMod, ModSearchResult } from "../types";
  import { ModListView, type MLEntry } from "./ModListView";
  let { autoMods, onSave, onClose }: { autoMods: AutoMod[]; onSave: (mods: AutoMod[]) => void; onClose: () => void } = $props();
  const { t } = createTranslation();
  let addingId = $state<string | null>(null);
  let justAdded = $state<Set<string>>(new Set());

  function toggleById(id: string) { onSave(autoMods.map((mod) => mod.project_id === id ? { ...mod, enabled: !mod.enabled } : mod)); }
  function deleteById(id: string) {
    onSave(autoMods.filter((mod) => mod.project_id !== id));
    const next = new Set(justAdded); next.delete(id); justAdded = next;
  }
  async function addFromSearch(mod: ModSearchResult): Promise<void> {
    if (autoMods.some((item) => item.project_id === mod.project_id) || justAdded.has(mod.project_id)) return;
    addingId = mod.project_id;
    const newMod: AutoMod = {
      project_id: mod.project_id, name: mod.title, description: mod.description, icon_url: mod.icon_url ?? null,
      enabled: true, tags: [], loaders: [], install_rank: 2, keep_priority: 50,
    };
    onSave([...autoMods, newMod]);
    justAdded = new Set([...justAdded, mod.project_id]); addingId = null;
  }
  function searchState(projectId: string): "idle" | "loading" | "done" {
    if (addingId === projectId) return "loading";
    if (justAdded.has(projectId) || autoMods.some((mod) => mod.project_id === projectId)) return "done";
    return "idle";
  }
  const entries = $derived<MLEntry[]>(autoMods.map((mod) => ({ id: mod.project_id, name: mod.name, icon: mod.icon_url, enabled: mod.enabled, subtitle: mod.description })));
</script>

<ModListView {onClose} searchPlaceholder={t("mods.rec_search_placeholder")} {entries} loading={false} onEntryClick={toggleById} onEntryDelete={deleteById} loader="" mcVersion="" {searchState} onInstall={addFromSearch}>
  {#snippet emptyNode()}
    <div class="py-8 px-4 text-center text-xs text-t3 leading-[1.6]">{t("mods.rec_empty")}<br /><span class="text-[10px]">{t("mods.rec_empty_hint")}</span></div>
  {/snippet}
  {#snippet renderEntryBadges(entry: MLEntry)}
    {@const mod = autoMods.find((item) => item.project_id === entry.id)}
    {#if mod}
      {#if mod.tags.includes("nvidia-only")}<span class="shrink-0 text-[9px] px-[5px] py-[1px] rounded-[3px] leading-[1.6]" style={css({ background: "rgba(184,144,48,.12)", color: C.warning })}>Nvidia</span>{/if}
      {#if mod.tags.includes("beta")}<span class="shrink-0 text-[9px] px-[5px] py-[1px] rounded-[3px] leading-[1.6]" style={css({ background: C.dangerBg, color: C.danger })}>β</span>{/if}
      {#if mod.tags.includes("opt-in")}<span class="shrink-0 text-[9px] px-[5px] py-[1px] rounded-[3px] leading-[1.6]" style={css({ background: C.hover, color: C.t3 })}>opt-in</span>{/if}
      {#if mod.tags.includes("unsupported-gpu")}<span class="shrink-0 text-[9px] px-[5px] py-[1px] rounded-[3px] leading-[1.6]" style={css({ background: C.hover, color: C.t3 })}>GPU</span>{/if}
      {#if mod.loaders.length === 0}<span class="shrink-0 text-[9px] px-[5px] py-[1px] rounded-[3px] leading-[1.6]" style={css({ background: C.hover, color: C.t3 })}>{t("mods.custom_badge")}</span>{/if}
    {/if}
  {/snippet}
</ModListView>
