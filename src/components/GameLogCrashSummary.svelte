<script lang="ts">
  import { Copy } from "@lucide/svelte";
  import { createTranslation } from "../lib/i18n.svelte";
  import type { ParsedCrash, Profile } from "../types";
  let { diagnosis, activeProfile, compact = false, copied, onCopy }: {
    diagnosis: ParsedCrash | null;
    activeProfile?: Profile;
    compact?: boolean;
    copied: boolean;
    onCopy: () => void;
  } = $props();
  const translation = createTranslation();
  const { t } = translation;
</script>

{#if diagnosis}
  <div class={compact ? "crash-summary compact" : "crash-summary"}>
    <div class="flex items-center gap-2">
      <span class="crash-chip">{diagnosis.diagnosis.category}</span>
      <span class="text-[10px] text-t3 font-mono">{Math.round(diagnosis.diagnosis.confidence * 100)}%</span>
      {#if activeProfile}<span class="text-[10px] text-t3 ml-auto">{activeProfile.mcVersion} / {activeProfile.loader}</span>{/if}
    </div>
    <p class="text-[12px] leading-[1.65] text-t1 mt-2">{diagnosis.diagnosis.summary}</p>
    {#if diagnosis.diagnosis.actions.length}
      <div class="flex flex-wrap gap-1.5 mt-2">{#each diagnosis.diagnosis.actions as action}<span class="crash-action" title={action.detail}>{action.label}</span>{/each}</div>
    {/if}
    {#if !compact}<div class="flex gap-1.5 mt-3"><button class="crash-action button" onclick={onCopy}><Copy size={11} /> {copied ? t("common.copied") : t("common.copy_all")}</button></div>{/if}
    {#if diagnosis.diagnosis.evidence.length}
      <details class="mt-2">
        <summary class="text-[10px] text-t3 cursor-pointer select-none">{t("log.evidence")}</summary>
        <div class="mt-1.5 flex flex-col gap-0.5">{#each diagnosis.diagnosis.evidence as line}<code class="crash-evidence">{line}</code>{/each}</div>
      </details>
    {/if}
  </div>
{:else}
  <div class="crash-summary"><p class="text-[12px] text-t2">{t("log.no_crash_analysis", { defaultValue: translation.language?.startsWith("ja") ? "クラッシュの解析結果はまだありません。" : "No crash analysis is available yet." })}</p></div>
{/if}
