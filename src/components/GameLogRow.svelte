<script lang="ts">
  import { assignRef } from "../lib/dom";
  import { C } from "../theme";
  import type { Level, LogLine } from "./GameLog.types";
  const LEVEL_STYLE: Record<Level, { color: string; bg: string; label: string }> = {
    INFO: { color: C.t3, bg: "transparent", label: "INFO" },
    DEBUG: { color: C.t3, bg: "transparent", label: "DEBUG" },
    OTHER: { color: C.t3, bg: "transparent", label: "" },
    WARN: { color: C.warning, bg: "rgba(184,144,48,.08)", label: "WARN" },
    ERROR: { color: C.danger, bg: "rgba(122,58,50,.12)", label: "ERROR" },
    FATAL: { color: C.danger, bg: "rgba(122,58,50,.16)", label: "FATAL" },
  };
  let { line, rows, highlighted }: { line: LogLine; rows: Map<number, HTMLDivElement>; highlighted: boolean } = $props();
  const style = $derived(LEVEL_STYLE[line.level]);
  function register(node: HTMLDivElement | null) {
    if (node) rows.set(line.id, node);
    else rows.delete(line.id);
  }
</script>

<div use:assignRef={register} class={highlighted ? "log-row highlighted" : "log-row"} style:background={style.bg}>
  <span class="log-line-no">{line.id + 1}</span>
  {#if line.time}<span class="log-time">{line.time}</span>{/if}
  {#if !["INFO", "OTHER", "DEBUG"].includes(line.level)}<span class="log-level" style:color={style.color}>{style.label}</span>{/if}
  <span class="break-all" style:color={style.color === C.t3 ? C.t2 : style.color}>{line.body || line.raw}</span>
</div>
