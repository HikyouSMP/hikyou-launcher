<script module lang="ts">
  import type { LoaderType } from "../types";
  export interface CreatingProfileDraft {
    versionId: string;
    versionType?: string;
    loader: LoaderType;
    inputName: string;
  }
</script>

<script lang="ts">
  import { C } from "../theme";
  import { assignRef, css } from "../lib/dom";
  import type { Ref } from "../lib/state";
  import type { VersionEntry } from "../types";
  import { isLoaderCompatible } from "../utils/versionCompatibility";
  let { versions, loaders, navItems, navIndex, hoverKey, navElemsRef, createInputRef, labelForLoader, onHoverKeyChange, onCreateDraft }: {
    versions: VersionEntry[]; loaders: LoaderType[]; navItems: string[]; navIndex: number; hoverKey: string | null;
    navElemsRef: Ref<Map<string, HTMLElement>>; createInputRef: Ref<HTMLInputElement | null>;
    labelForLoader: (loader: LoaderType | string) => string; onHoverKeyChange: (key: string | null) => void; onCreateDraft: (draft: CreatingProfileDraft) => void;
  } = $props();
  function startCreate(version: VersionEntry, loader: LoaderType) {
    onCreateDraft({ versionId: version.id, versionType: version.type, loader, inputName: "" });
    setTimeout(() => createInputRef.current?.focus(), 30);
  }
</script>

{#each versions as version (version.id)}
  {#each loaders as loader (loader)}
    {#if isLoaderCompatible(loader, version.id)}
      {@const navKey = `c:${version.id}:${loader}`}
      {@const focused = navItems[navIndex] === navKey}
      <div role="button" tabindex="0" use:assignRef={(element: HTMLElement | null) => { if (element) navElemsRef.current.set(navKey, element); else navElemsRef.current.delete(navKey); }}
        data-command-selected={focused}
        onclick={() => startCreate(version, loader)} onkeydown={(event) => { if (event.key === "Enter") startCreate(version, loader); }}
        style={css({ display: "flex", alignItems: "center", gap: 8, padding: "9px 12px", margin: "0 6px 1px", borderRadius: C.r, cursor: "pointer", border: "1px solid transparent", background: focused || hoverKey === navKey ? C.hover : "transparent", outline: "none" })}
        onmousemove={(event) => { if (event.movementX || event.movementY) onHoverKeyChange(navKey); }} onmouseleave={() => onHoverKeyChange(null)}>
        <div style="flex:1;min-width:0"><p style={css({ fontSize: 13, fontWeight: 400, color: C.t1, fontFamily: "'Inter',system-ui,sans-serif" })}>{labelForLoader(loader)} {version.id}</p></div>
      </div>
    {/if}
  {/each}
{/each}
