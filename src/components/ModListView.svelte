<script lang="ts">
  import { onMount } from "svelte";
  import { ArrowLeft, Download, History, Search } from "@lucide/svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { createTranslation } from "../lib/i18n.svelte";
  import { css } from "../lib/dom";
  import { C } from "../theme";
  import type { ModSearchResult } from "../types";
  import type { ModListViewProps } from "./ModListView.types";
  import ModIcon from "./ModIcon.svelte";
  import ModHistory from "./ModHistory.svelte";

  let { onClose, embedded = false, searchPlaceholder, headerRight, entries, loading, emptyNode, errorNode,
    onEntryClick, onEntryDelete, loader, mcVersion, searchState, onInstall, onSearchResultDelete,
    renderSearchMeta, renderEntryBadges, profileId, onHistoryRestored }: ModListViewProps & {
      profileId?: string;
      onHistoryRestored?: () => void | Promise<void>;
    } = $props();
  const { t } = createTranslation();
  let query = $state("");
  let historyOpen = $state(false);
  let results = $state<ModSearchResult[]>([]);
  let searching = $state(false);
  let searchErr = $state<string | null>(null);
  let kbIdx = $state(0);
  let hoverIdx = $state<number | null>(null);
  let pendingDelete = $state<string | null>(null);
  let pendingDeleteSearch = $state<string | null>(null);
  let timer: ReturnType<typeof setTimeout> | null = null;
  let scrollTimer: ReturnType<typeof setTimeout> | null = null;
  let list = $state<HTMLDivElement>();
  let input = $state<HTMLInputElement>();
  const itemRefs = new Map<number, HTMLElement>();
  let request = 0;
  const isSearchMode = $derived(query.trim().length > 0);

  function itemRef(node: HTMLElement, index: number) {
    itemRefs.set(index, node);
    return {
      update(next: number) {
        if (itemRefs.get(index) === node) itemRefs.delete(index);
        index = next; itemRefs.set(index, node);
      },
      destroy() { if (itemRefs.get(index) === node) itemRefs.delete(index); },
    };
  }
  function cancelDelete() { pendingDelete = null; pendingDeleteSearch = null; }
  function clearSearch() {
    request++;
    if (timer) clearTimeout(timer);
    query = ""; results = []; searching = false; searchErr = null; kbIdx = 0; hoverIdx = null;
  }
  function handleListScroll() {
    hoverIdx = null;
    if (list) list.style.pointerEvents = "none";
    if (scrollTimer) clearTimeout(scrollTimer);
    scrollTimer = setTimeout(() => { if (list) list.style.pointerEvents = ""; }, 150);
  }
  async function runSearch(q: string, current: number) {
    if (!q.trim()) { results = []; kbIdx = 0; searching = false; return; }
    searching = true; searchErr = null;
    try {
      const found = await invoke<ModSearchResult[]>("search_modrinth", { query: q.trim(), loader, mcVersion });
      if (current !== request) return;
      results = found; kbIdx = 0; hoverIdx = null;
      requestAnimationFrame(() => itemRefs.get(0)?.scrollIntoView({ block: "nearest" }));
    } catch (reason) { if (current === request) searchErr = String(reason); }
    finally { if (current === request) searching = false; }
  }
  function handleQueryChange(value: string) {
    query = value;
    const current = ++request;
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => void runSearch(value, current), 350);
  }
  function deleteSearch(mod: ModSearchResult) {
    if (onSearchResultDelete) onSearchResultDelete(mod); else onEntryDelete(mod.project_id);
    pendingDeleteSearch = null;
  }
  function handleKeydown(event: KeyboardEvent) {
    if (historyOpen) return;
    if (event.key !== "Escape" && event.target instanceof Element && event.target.closest("button, select, a")) return;
    if (event.key === "Escape") {
      event.stopPropagation();
      if (pendingDelete || pendingDeleteSearch) { cancelDelete(); return; }
      if (query.trim()) { clearSearch(); return; }
      onClose(); return;
    }
    const count = isSearchMode ? results.length : entries.length;
    if (event.altKey) {
      event.preventDefault();
      if (event.key === "ArrowRight") {
        if (pendingDelete || pendingDeleteSearch) { cancelDelete(); return; }
        if (isSearchMode && kbIdx >= 0 && kbIdx < results.length) {
          const mod = results[kbIdx]; if (searchState(mod.project_id) === "idle") void onInstall(mod);
        }
      } else if (event.key === "ArrowLeft") {
        if (isSearchMode) {
          if (kbIdx >= 0 && kbIdx < results.length) {
            const mod = results[kbIdx];
            if (searchState(mod.project_id) === "done") {
              if (pendingDeleteSearch === mod.project_id) deleteSearch(mod);
              else pendingDeleteSearch = mod.project_id;
            }
          }
        } else if (kbIdx >= 0 && kbIdx < entries.length) {
          const entry = entries[kbIdx];
          if (pendingDelete === entry.id) { onEntryDelete(entry.id); pendingDelete = null; }
          else pendingDelete = entry.id;
        }
      }
      return;
    }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      if (!count) return;
      event.preventDefault();
      if (input) { const pos = input.selectionEnd ?? input.value.length; input.setSelectionRange(pos, pos); }
      const current = kbIdx;
      if (event.key === "ArrowDown") kbIdx = current >= count - 1 ? (event.repeat ? count - 1 : 0) : current < 0 ? 0 : current + 1;
      else kbIdx = current <= 0 ? (current < 0 ? count - 1 : event.repeat ? 0 : count - 1) : current - 1;
      hoverIdx = null; return;
    }
    if (kbIdx < 0 || kbIdx >= count) return;
    if (event.key === "Enter") {
      event.preventDefault();
      if (isSearchMode) {
        const mod = results[kbIdx]; const state = searchState(mod.project_id);
        if (state === "done" && pendingDeleteSearch === mod.project_id) deleteSearch(mod);
        else if (state === "idle") void onInstall(mod);
      } else {
        const entry = entries[kbIdx];
        if (pendingDelete === entry.id) { onEntryDelete(entry.id); pendingDelete = null; }
        else onEntryClick?.(entry.id);
      }
    }
  }
  $effect(() => { const index = kbIdx; cancelDelete(); if (index >= 0) itemRefs.get(index)?.scrollIntoView({ block: "nearest" }); });
  onMount(() => {
    const focusTimer = setTimeout(() => input?.focus(), 60);
    window.addEventListener("keydown", handleKeydown, true);
    return () => {
      window.removeEventListener("keydown", handleKeydown, true);
      request++; clearTimeout(focusTimer);
      if (timer) clearTimeout(timer);
      if (scrollTimer) clearTimeout(scrollTimer);
    };
  });
  function fmtDl(n: number) {
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 1_000) return `${(n / 1000).toFixed(0)}K`;
    return String(n);
  }
</script>

{#if historyOpen && profileId}
  <ModHistory {profileId} onClose={() => { historyOpen = false; requestAnimationFrame(() => input?.focus()); }} onRestored={onHistoryRestored} />
{:else}
<div class="flex-1 flex flex-col overflow-hidden">
  <div class={`flex items-center px-[10px] h-[52px] shrink-0 gap-2 ${embedded ? "" : "border-b border-b1"}`}>
    {#if !embedded}
      <button onclick={onClose} class="flex items-center px-[6px] py-1 rounded-[6px] cursor-pointer transition-colors duration-[120ms] bg-transparent border-0 text-t3" onmouseenter={(event) => event.currentTarget.style.color = C.t1} onmouseleave={(event) => event.currentTarget.style.color = C.t3}><ArrowLeft size={14} /></button>
    {/if}
    <Search size={14} class="text-t3 shrink-0" />
    <input bind:this={input} value={query} oninput={(event) => handleQueryChange(event.currentTarget.value)} placeholder={searchPlaceholder} class="flex-1 bg-transparent border-none outline-none text-sm text-t1 font-light font-[inherit]" />
    {#if query}<button onclick={() => { clearSearch(); input?.focus(); }} class="cursor-pointer text-t3 text-base leading-none px-[2px] bg-transparent border-0">×</button>{/if}
    {@render headerRight?.()}
    {#if profileId}<button class="icon-btn shrink-0" title={t("mod_history.title")} onclick={() => historyOpen = true}><History size={14} /></button>{/if}
  </div>
  {@render errorNode?.()}
  {#if searchErr}<div class="px-4 py-[6px] text-[11px] text-danger">{searchErr}</div>{/if}
  <div bind:this={list} class="sb flex-1 overflow-y-auto py-[6px]" onscroll={handleListScroll}>
    {#if isSearchMode}
      {#if searching}<div class="py-8 text-center text-xs text-t3">{t("mods.searching")}</div>
      {:else}
        {#if results.length === 0}<div class="py-8 px-4 text-center text-xs text-t3">{t("mods.no_search_result", { query })}</div>{/if}
        {#each results as mod, idx (mod.project_id)}
          {@const state = searchState(mod.project_id)}
          {@const isPendingDel = pendingDeleteSearch === mod.project_id && state === "done"}
          {@const isActive = kbIdx === idx || hoverIdx === idx}
          {@const bg = isPendingDel && isActive ? C.dangerBg : kbIdx === idx ? C.hover : hoverIdx === idx ? C.hoverLight : "transparent"}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div use:itemRef={idx} onmouseenter={() => hoverIdx = idx} onmousemove={() => { if (hoverIdx !== idx) hoverIdx = idx; }} onmouseleave={() => hoverIdx = null} class="flex items-center px-3 gap-[10px] rounded-[6px] mx-[6px] mb-[1px] h-11 box-border overflow-hidden" style={css({ background: bg, scrollMarginTop: 8, scrollMarginBottom: 8, transition: "none" })}>
            <ModIcon url={mod.icon_url} name={mod.title} size={28} />
            <div class="flex-1 min-w-0">
              {#if isPendingDel}<div class="text-[13px] text-danger">{t("mods.delete_confirm")}</div>
              {:else}
                <div class="text-[13px] text-t1 whitespace-nowrap overflow-hidden text-ellipsis">{mod.title}</div>
                {#if renderSearchMeta}{@render renderSearchMeta(mod)}{:else}<div class="text-[10px] text-t3 mt-[1px]">↓ {fmtDl(mod.downloads)}</div>{/if}
              {/if}
            </div>
            {#if state !== "done"}
              <button onclick={() => state === "idle" && onInstall(mod)} disabled={state === "loading"} title={t("mods.install_add")} class="rounded-[6px] w-7 h-7 flex items-center justify-center shrink-0" style={css({ background: isActive ? C.greenBg : "transparent", border: "none", color: isActive ? C.green : C.t3, cursor: state === "loading" ? "default" : "pointer", opacity: state === "loading" ? 0.5 : 1, transition: "none" })}>
                {#if state === "loading"}…{:else}<Download size={13} />{/if}
              </button>
            {:else}
              <button onclick={() => { if (isPendingDel) deleteSearch(mod); else pendingDeleteSearch = mod.project_id; }} title={t("mods.install_delete")} class="rounded-[6px] w-7 h-7 flex items-center justify-center shrink-0 cursor-pointer text-[13px] px-1 py-[2px] leading-none" style={css({ background: "none", border: "none", color: isPendingDel ? C.danger : isActive ? C.t3 : "transparent", transition: "color .1s", pointerEvents: isActive ? "auto" : "none" })} onmouseenter={(event) => event.currentTarget.style.color = C.danger} onmouseleave={(event) => event.currentTarget.style.color = isPendingDel ? C.danger : C.t3}>✕</button>
            {/if}
          </div>
        {/each}
      {/if}
    {:else if loading}<div class="py-8 text-center text-xs text-t3">{t("mods.loading")}</div>
    {:else}
      {#if entries.length === 0}
        {#if emptyNode}{@render emptyNode()}{:else}<div class="py-8 px-4 text-center text-xs text-t3 leading-[1.6]">{t("mods.empty")}<br /><span class="text-[10px]">{t("mods.empty_hint")}</span></div>{/if}
      {/if}
      {#each entries as entry, idx (entry.id)}
        {@const isPendingDel = pendingDelete === entry.id}
        {@const isActive = kbIdx === idx || hoverIdx === idx}
        {@const bg = isPendingDel && isActive ? C.dangerBg : kbIdx === idx ? C.hover : hoverIdx === idx ? C.hoverLight : "transparent"}
        <!-- Keyboard actions are handled by the capture listener above. -->
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <div data-mod-entry={entry.id} use:itemRef={idx} onclick={() => onEntryClick?.(entry.id)} onmouseenter={() => hoverIdx = idx} onmousemove={() => { if (hoverIdx !== idx) hoverIdx = idx; }} onmouseleave={() => hoverIdx = null} class="flex items-center px-3 gap-[10px] rounded-[6px] mx-[6px] mb-[1px] h-10 box-border overflow-hidden" style={css({ background: bg, scrollMarginTop: 8, scrollMarginBottom: 8, transition: "none", cursor: onEntryClick ? "pointer" : "default" })}>
          <ModIcon url={entry.icon} name={entry.name} size={28} faded={!entry.enabled} />
          <div class="flex-1 min-w-0 transition-opacity duration-200" style:opacity={entry.enabled ? 1 : 0.4}>
            {#if isPendingDel}<div class="text-[13px] text-danger whitespace-nowrap overflow-hidden text-ellipsis">{t("mods.delete_confirm")}</div>
            {:else}
              <div class="flex items-center gap-1">
                <span class="text-[13px] text-t1 whitespace-nowrap overflow-hidden text-ellipsis">{entry.name}</span>
                {@render entry.badges?.()}
                {@render renderEntryBadges?.(entry)}
              </div>
              {#if entry.subtitle}<div class="text-[9px] text-t3 mt-[1px] whitespace-nowrap overflow-hidden text-ellipsis" style="font-family: 'JetBrains Mono','SF Mono',monospace">{entry.subtitle}</div>{/if}
            {/if}
          </div>
          <button onclick={(event) => { event.stopPropagation(); if (isPendingDel) { onEntryDelete(entry.id); pendingDelete = null; } else pendingDelete = entry.id; }} title={t("mods.install_delete")} class="rounded-[6px] cursor-pointer text-[13px] px-1 py-[2px] leading-none" style={css({ background: "none", border: "none", color: isPendingDel ? C.danger : isActive ? C.t3 : "transparent", transition: "color .1s, opacity .1s", pointerEvents: isActive ? "auto" : "none" })} onmouseenter={(event) => event.currentTarget.style.color = C.danger} onmouseleave={(event) => event.currentTarget.style.color = isPendingDel ? C.danger : C.t3}>✕</button>
        </div>
      {/each}
    {/if}
    {#if !isSearchMode && entries.length > 0}<div class="px-3 pt-2 pb-1 text-[9px] text-t3 text-center opacity-60">{t("mods.kb_hint_installed")}</div>{/if}
    {#if isSearchMode && results.length > 0}<div class="px-3 pt-2 pb-1 text-[9px] text-t3 text-center opacity-60">{t("mods.kb_hint_search")}</div>{/if}
  </div>
</div>
{/if}
