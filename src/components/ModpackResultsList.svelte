<script lang="ts">
  import { Download } from "@lucide/svelte";
  import { createTranslation } from "../lib/i18n.svelte";
  import { assignRef, css } from "../lib/dom";
  import type { Ref } from "../lib/state";
  import { C } from "../theme";
  import type { ModSearchResult } from "../types";
  let { results, searching, searchValue, navItems, navIndex, hoveredIndex, navElemsRef, onHoverIndexChange, onOpenVersionDialog }: {
    results: ModSearchResult[]; searching: boolean; searchValue: string; navItems: string[]; navIndex: number; hoveredIndex: number | null;
    navElemsRef: Ref<Map<string, HTMLElement>>; onHoverIndexChange: (index: number | null) => void; onOpenVersionDialog: (modpack: ModSearchResult) => void;
  } = $props();
  const { t } = createTranslation();
  const formatDownloads = (count: number) => count >= 1_000_000 ? `${(count / 1_000_000).toFixed(1)}M` : count >= 1_000 ? `${(count / 1000).toFixed(0)}K` : String(count);
</script>

{#if searching}
  <div class="px-4 py-3 text-t3 text-[12px]">{t("common.searching")}</div>
{:else if !searchValue.trim()}
  <div class="py-7.5 px-6 text-center"><p class="text-[12px] text-t3 leading-[1.7]">{t("modpack.search_hint")}</p></div>
{:else if results.length === 0}
  <div class="py-7.5 px-6 text-center"><p class="text-[12px] text-t3">{t("modpack.no_match", { query: searchValue })}</p></div>
{:else}
  {#each results as modpack, index (`${modpack.project_id}:${index}`)}
    {@const navKey = `modpack:${index}`}
    {@const focused = navItems[navIndex] === navKey}
    {@const hovered = hoveredIndex === index}
    <div use:assignRef={(element: HTMLElement | null) => { if (element) navElemsRef.current.set(navKey, element); else navElemsRef.current.delete(navKey); }}
      data-command-selected={focused}
      onmousemove={(event) => { if (event.movementX || event.movementY) onHoverIndexChange(index); }} onmouseleave={() => onHoverIndexChange(null)} onclick={() => onOpenVersionDialog(modpack)}
      role="button" tabindex="0" onkeydown={(event) => { if (event.key === "Enter") onOpenVersionDialog(modpack); }}
      class="flex items-center gap-2.5 px-3 py-1.5 mx-1.5 mb-px rounded-md min-h-11 cursor-pointer" style={css({ background: focused ? C.hover : hovered ? C.hoverLight : "transparent" })}>
      {#if modpack.icon_url}
        <img src={modpack.icon_url} alt="" width={28} height={28} class="rounded-md shrink-0 object-cover block" onerror={(event) => { (event.currentTarget as HTMLImageElement).style.display = "none"; }} />
      {:else}
        <div class="w-7 h-7 rounded-md shrink-0 flex items-center justify-center text-[13px] text-t3 font-semibold" style="background:rgba(255,255,255,.055)">{(modpack.title || "?")[0].toUpperCase()}</div>
      {/if}
      <div class="flex-1 min-w-0"><div class="text-[13px] text-t1 whitespace-nowrap overflow-hidden text-ellipsis">{modpack.title}</div><div class="text-[10px] text-t3 mt-px">↓ {formatDownloads(modpack.downloads)}</div></div>
      <button onclick={(event) => { event.stopPropagation(); onOpenVersionDialog(modpack); }} title={t("modpack.select_version_btn")} class="w-7 h-7 flex items-center justify-center rounded-md cursor-pointer shrink-0"
        style={css({ background: focused ? C.greenBg : "transparent", border: "none", color: focused ? C.green : C.t3 })}><Download size={13} /></button>
    </div>
  {/each}
{/if}
