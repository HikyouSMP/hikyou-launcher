<script module lang="ts">
  export interface InstallingModpackVersion { projectId: string; versionId: string; title?: string }
</script>

<script lang="ts">
  import { Download } from "@lucide/svelte";
  import { createTranslation } from "../lib/i18n.svelte";
  import { css } from "../lib/dom";
  import { C } from "../theme";
  import type { ModSearchResult, ModpackVersionInfo } from "../types";
  import ModalBackdrop from "./ModalBackdrop.svelte";
  let { modpack, versions, isLoading, installing, focusedIndex, hoveredIndex, onHoveredIndexChange, onClose, onInstall }: {
    modpack: ModSearchResult; versions: ModpackVersionInfo[]; isLoading: boolean; installing: InstallingModpackVersion | null; focusedIndex: number; hoveredIndex: number | null;
    onHoveredIndexChange: (index: number | null) => void; onClose: () => void; onInstall: (version: ModpackVersionInfo) => void;
  } = $props();
  const { t } = createTranslation();
  let anyInstalling = $derived(installing !== null);
</script>

<div style={css({ position: "fixed", inset: 0, zIndex: 200, display: "flex", alignItems: "center", justifyContent: "center" })} onclick={anyInstalling ? undefined : onClose} role="presentation">
  <ModalBackdrop fixed />
  <div onclick={(event) => event.stopPropagation()} onkeydown={(event) => { if (event.key === "Escape" && !anyInstalling) { event.stopPropagation(); onClose(); } }} role="dialog" aria-modal="true" aria-label={modpack.title} tabindex="-1"
    style={css({ position: "relative", zIndex: 1, width: 268, maxHeight: 340, background: "rgba(34,33,31,.72)", backdropFilter: "blur(24px) saturate(155%)", WebkitBackdropFilter: "blur(24px) saturate(155%)", borderRadius: C.r + 2, display: "flex", flexDirection: "column", overflow: "hidden", boxShadow: "0 12px 40px rgba(0,0,0,.5)" })}>
    <div style={css({ padding: "11px 13px 9px", borderBottom: `1px solid ${C.b2}`, flexShrink: 0 })}>
      <div style={css({ fontSize: 13, fontWeight: 500, color: C.t1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", marginBottom: 1 })}>{modpack.title}</div>
      <div style={css({ fontSize: 10, color: C.t3 })}>{t("modpack.select_version")}</div>
    </div>
    <div class="sb" style="overflow-y:auto;flex:1;padding:6px 0">
      {#if isLoading}<div style={css({ padding: "20px", textAlign: "center", fontSize: 11, color: C.t3 })}>{t("common.fetching")}</div>
      {:else if versions.length === 0}<div style={css({ padding: "20px", textAlign: "center", fontSize: 11, color: C.t3 })}>{t("common.not_found")}</div>
      {:else}
        {#each versions as version, index (version.id)}
          {@const isInstalling = installing?.projectId === modpack.project_id && installing?.versionId === version.id}
          {@const focused = focusedIndex === index}
          {@const hovered = hoveredIndex === index}
          <div data-modpack-version-idx={index} onclick={() => onInstall(version)} onmouseenter={() => onHoveredIndexChange(index)} onmouseleave={() => onHoveredIndexChange(null)}
            role="button" tabindex="0" onkeydown={(event) => { if (event.key === "Enter") onInstall(version); }}
            style={css({ display: "flex", alignItems: "center", gap: 10, padding: "6px 12px", margin: "0 6px 1px", borderRadius: C.r, cursor: anyInstalling ? "default" : "pointer", background: hovered || focused ? C.hover : "transparent", opacity: anyInstalling && !isInstalling ? 0.4 : 1, minHeight: 40, scrollMarginTop: 8, scrollMarginBottom: 8 })}>
            <div style="flex:1;min-width:0">
              <div style={css({ fontSize: 13, color: C.t1, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" })}>{version.version_number}</div>
              <div style={css({ display: "flex", gap: 3, marginTop: 2, flexWrap: "wrap" })}>
                {#each version.game_versions.slice(0, 3) as gameVersion (gameVersion)}<span style={css({ fontSize: 9, padding: "1px 5px", borderRadius: 4, background: "rgba(255,255,255,.06)", color: C.t3, lineHeight: 1.6 })}>{gameVersion}</span>{/each}
              </div>
            </div>
            {#if isInstalling}<div style={css({ width: 14, height: 14, borderRadius: "50%", border: `1.5px solid ${C.t3}`, borderTopColor: C.green, animation: "spin .7s linear infinite", flexShrink: 0 })}></div>
            {:else if (hovered || focused) && !anyInstalling}<Download size={13} style={`color:${C.green};flex-shrink:0`} />{/if}
          </div>
        {/each}
      {/if}
    </div>
  </div>
</div>
