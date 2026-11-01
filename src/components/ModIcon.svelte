<script lang="ts">
  import { untrack } from "svelte";
  import { createTranslation } from "../lib/i18n.svelte";
  import { useModIcon } from "../hooks/useModIcon.svelte";
  import { C } from "../theme";
  import { css } from "../lib/dom";
  let { url, projectId, name, size = 28, faded = false }: { url?: string | null; projectId?: string | null; name: string; size?: number; faded?: boolean } = $props();
  const { t } = createTranslation();
  const icon = useModIcon();
  $effect(() => {
    const source = url; const project = projectId;
    untrack(() => { void icon.load(source, project); });
    return () => icon.dispose();
  });
  const letter = $derived((name || "?")[0].toUpperCase());
  const baseStyle = $derived({ width: size, height: size, borderRadius: 6, flexShrink: 0, display: "block", opacity: faded ? 0.35 : 1, transition: "opacity .2s" });
</script>

{#if icon.src}
  {@const source = icon.src}
  {@const request = icon.request}
  {#key request}<img src={source} alt="" title={icon.cacheWarning ? t("mods.icon_cache_warning") : undefined} onerror={() => icon.imageFailed(source, request)} style={css({ ...baseStyle, objectFit: "cover" })} />{/key}
{:else}
  <div role="img" aria-label={t(`mods.icon_${icon.status}`, { name })} title={t(`mods.icon_${icon.status}`, { name })} style={css({ ...baseStyle, background: C.surface, display: "flex", alignItems: "center", justifyContent: "center", fontSize: size * 0.45, color: C.t2, fontWeight: 500, userSelect: "none" })}>{letter}</div>
{/if}
