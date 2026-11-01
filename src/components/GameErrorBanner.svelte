<script lang="ts">
  import { FileText, X } from "@lucide/svelte";
  import LaunchReportDialog from "./LaunchReportDialog.svelte";
  import { createTranslation } from "../lib/i18n.svelte";
  import { C } from "../theme";
  let { error, onClose }: { error: string; onClose: () => void } = $props();
  const { t } = createTranslation();
  let reportOpen = $state(false);
  const message = $derived((error.match(/message:\s*([^,}]+)/i)?.[1] ?? error).replace(/^Error:\s*/i, "").trim());
</script>

<div style:margin="4px 8px" style:border-radius={`${C.r}px`} style:background={C.dangerBg} style:border={`1px solid ${C.dangerBdr}`}>
  <div style="display:flex;align-items:center;justify-content:space-between;padding:8px 12px 4px">
    <span style="color:rgba(248,113,113,.9);font-size:11px;font-weight:700;letter-spacing:.04em">{t("game_error.title")}</span>
    <button onclick={onClose} title={t("common.close")} style="background:none;border:none;color:rgba(248,113,113,.5);cursor:pointer;line-height:1;user-select:none"><X size={16} /></button>
  </div>
  <p style="padding:0 12px 10px;color:rgba(248,113,113,.75);font-size:10px;line-height:1.65;word-break:break-all;font-family:'JetBrains Mono','SF Mono','Fira Code',monospace">{message}</p>
  <button class="report-action" type="button" data-command-action onkeydown={event => event.stopPropagation()} onclick={() => { reportOpen = true; }}><FileText size={13} />{t("launch_report.affordance")}</button>
</div>

{#if reportOpen}{#key error}<LaunchReportDialog {error} onClose={() => { reportOpen = false; }} />{/key}{/if}

<style>
  .report-action { display: inline-flex; align-items: center; gap: 5px; min-height: 28px; margin: 0 8px 6px; padding: 4px; border: 0; border-radius: 4px; background: transparent; color: var(--color-t2); font-size: 11px; cursor: pointer; }
  .report-action:hover { background: rgba(255,255,255,.05); color: var(--color-t1); }
  .report-action:focus-visible { outline: 2px solid var(--color-t2); outline-offset: 2px; }
</style>
