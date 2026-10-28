<script module lang="ts">
  export const SLOW_STAGE_MS = 2000;
  export const SLOW_TOTAL_MS = 5000;
  export function formatMs(ms: number) {
    return ms >= 1000 ? `${(ms / 1000).toFixed(ms >= 10000 ? 1 : 2)} s` : `${ms} ms`;
  }
</script>

<script lang="ts">
  import { createTranslation } from "../lib/i18n.svelte";
  import type { LaunchMetrics } from "../types";
  let { metrics }: { metrics: LaunchMetrics } = $props();
  const { t } = createTranslation();
  const maxStageMs = $derived(Math.max(1, ...metrics.stages.map((stage) => stage.ms)));
  const slowest = $derived(metrics.stages.reduce<LaunchMetrics["stages"][number] | null>((current, stage) => !current || stage.ms > current.ms ? stage : current, null));
  const stageRows = $derived(metrics.stages.filter((stage) => stage.ms > 0));
</script>

<div class="debug-metrics">
  <div class="debug-metric-head">
    <div class="min-w-0">
      <div class="debug-metric-profile" title={metrics.profileId}>{metrics.profileId}</div>
      <div class="debug-metric-version">{t("debug.launch_version")} {metrics.versionId}</div>
    </div>
    <div class="debug-metric-total">
      <span>{t("debug.launch_total_pre_spawn")}</span>
      <b class={metrics.totalPreSpawnMs >= SLOW_TOTAL_MS ? "is-warn" : "is-good"}>{formatMs(metrics.totalPreSpawnMs)}</b>
    </div>
  </div>
  {#if slowest}
    <div class="debug-metric-insights"><span>{t("debug.launch_slowest_stage")}
      <b class={slowest.ms >= SLOW_STAGE_MS ? "is-warn" : ""}>{t(`debug.launch_stage_${slowest.name}`, { defaultValue: slowest.name })} {formatMs(slowest.ms)}</b>
    </span></div>
  {/if}
  <div class="debug-stage-grid">
    {#each stageRows as stage}
      {@const ratio = Math.max(4, Math.round((stage.ms / maxStageMs) * 100))}
      <div class={`debug-stage-pill ${stage.ms >= SLOW_STAGE_MS ? "is-warn" : ""} ${stage.name === slowest?.name ? "is-slowest" : ""}`}
        title={`${t(`debug.launch_stage_${stage.name}`, { defaultValue: stage.name })}: ${formatMs(stage.ms)}`}>
        <div class="debug-stage-line"><span>{t(`debug.launch_stage_${stage.name}`, { defaultValue: stage.name })}</span><b>{formatMs(stage.ms)}</b></div>
        <div class="debug-stage-track"><i style:width={`${ratio}%`}></i></div>
      </div>
    {/each}
  </div>
</div>
