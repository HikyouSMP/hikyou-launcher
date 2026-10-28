<script lang="ts">
  import { X } from "@lucide/svelte";
  import { createTranslation } from "../lib/i18n.svelte";
  import { C } from "../theme";
  let { error, onClose }: { error: string; onClose: () => void } = $props();
  const { t } = createTranslation();
  const message = $derived((error.match(/message:\s*([^,}]+)/i)?.[1] ?? error).replace(/^Error:\s*/i, "").trim());
</script>

<div style:margin="4px 8px" style:border-radius={`${C.r}px`} style:background={C.dangerBg} style:border={`1px solid ${C.dangerBdr}`}>
  <div style="display:flex;align-items:center;justify-content:space-between;padding:8px 12px 4px">
    <span style="color:rgba(248,113,113,.9);font-size:11px;font-weight:700;letter-spacing:.04em">{t("game_error.title")}</span>
    <button onclick={onClose} title={t("common.close")} style="background:none;border:none;color:rgba(248,113,113,.5);cursor:pointer;line-height:1;user-select:none"><X size={16} /></button>
  </div>
  <p style="padding:0 12px 10px;color:rgba(248,113,113,.75);font-size:10px;line-height:1.65;word-break:break-all;font-family:'JetBrains Mono','SF Mono','Fira Code',monospace">{message}</p>
</div>
