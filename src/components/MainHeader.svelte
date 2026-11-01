<script lang="ts">
  import { Search, Settings, Terminal } from "@lucide/svelte";
  import { createTranslation } from "../lib/i18n.svelte";
  import { assignRef, css } from "../lib/dom";
  import type { Ref } from "../lib/state";
  import type { VersionChannel } from "../types";
  import { C } from "../theme";
  import { Pill } from "./ui";
  import SearchModeChip from "./SearchModeChip.svelte";
  import CaretShortcut from "./CaretShortcut.svelte";
  import type { SearchSuggestion as Suggestion, SearchSuggestionAction } from "../search/suggestions";
  let { inputRef, searchValue, searchMode, versionChannel, debugVisible, activeDebug, canOpenLogInspector, onSearchChange, onOpenLogInspector, onToggleDebug, onOpenSettings, suggestion, onSuggestionAction }: {
    inputRef: Ref<HTMLInputElement | null>; searchValue: string; searchMode: "profile" | "modpack"; versionChannel: VersionChannel; debugVisible: boolean; activeDebug: boolean; canOpenLogInspector: boolean;
    onSearchChange: (value: string) => void; onOpenLogInspector: () => void; onToggleDebug: () => void; onOpenSettings: () => void;
    suggestion: Suggestion | null; onSuggestionAction: (action: SearchSuggestionAction) => void;
  } = $props();
  const { t } = createTranslation();
  let searchInput = $state<HTMLInputElement>();
</script>

<div data-tauri-drag-region class="flex items-center gap-2 px-3 h-13 shrink-0" style={css({ borderBottom: `1px solid ${C.b1}` })}>
  <Search size={16} class="text-t3 shrink-0" />
  {#if searchMode === "modpack"}<SearchModeChip label="Modpack" />{/if}
  <div class="relative flex flex-1 min-w-0 h-7 items-center overflow-hidden" data-search-input-shell>
    <input bind:this={searchInput} use:assignRef={inputRef} value={searchValue} oninput={(event) => onSearchChange(event.currentTarget.value)} placeholder={t("nav.search_placeholder")} class="flex-1 min-w-0 bg-transparent border-none outline-none text-[15px] text-t1 font-normal font-[inherit]" />
    <CaretShortcut input={searchInput} value={searchValue} {suggestion} onAction={onSuggestionAction} />
  </div>
  {#if searchMode === "profile" && versionChannel === "snapshot"}<SearchModeChip label={t("nav.snapshot_channel")} animated />{/if}
  {#if canOpenLogInspector}<Pill quiet onClick={onOpenLogInspector} title={t("log.title")}><Terminal size={13} /></Pill>{/if}
  {#if debugVisible}
    <button class="action-button is-quiet h-7 px-2 text-[11px] font-semibold font-mono" title={t("nav.debug_title")} aria-pressed={activeDebug} onclick={onToggleDebug}>DEBUG</button>
  {/if}
  <Pill quiet onClick={onOpenSettings} title={t("common.settings")}><Settings size={13} /></Pill>
</div>
