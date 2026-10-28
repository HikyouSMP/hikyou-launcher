<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { C } from "../theme";
  import { css } from "../lib/dom";
  let { url, name, size = 28, faded = false }: { url?: string | null; name: string; size?: number; faded?: boolean } = $props();
  let src = $state<string | null>(null);
  let err = $state(false);
  $effect(() => {
    let active = true;
    src = url ?? null; err = false;
    if (url) invoke<string>("fetch_cached_icon", { url }).then((dataUrl) => { if (active) src = dataUrl; }).catch(() => {});
    return () => { active = false; };
  });
  const letter = $derived((name || "?")[0].toUpperCase());
  const baseStyle = $derived({ width: size, height: size, borderRadius: 6, flexShrink: 0, display: "block", opacity: faded ? 0.35 : 1, transition: "opacity .2s" });
</script>

{#if src && !err}
  <img {src} alt="" onerror={() => err = true} style={css({ ...baseStyle, objectFit: "cover" })} />
{:else}
  <div style={css({ ...baseStyle, background: C.surface, display: "flex", alignItems: "center", justifyContent: "center", fontSize: size * 0.45, color: C.t3, fontWeight: 600, userSelect: "none" })}>{letter}</div>
{/if}
