<script lang="ts">
  import { X } from "@lucide/svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { createTranslation } from "../lib/i18n.svelte";
  import type { CrashAnalysis } from "../types";
  let { analysis, profileName, feedbackOpen, onFeedbackOpenChange, onClose, onCopyReport }: {
    analysis: CrashAnalysis;
    profileName: string;
    feedbackOpen: boolean;
    onFeedbackOpenChange: (open: boolean) => void;
    onClose: () => void;
    onCopyReport: () => Promise<void>;
  } = $props();
  const { t } = createTranslation();
</script>

<div class="crash-notice" data-selectable>
  <div class="flex items-center gap-2 min-w-0">
    <span class="crash-chip">{analysis.parsed.diagnosis.category}</span>
    <span class="crash-confidence">{Math.round(analysis.parsed.diagnosis.confidence * 100)}%</span>
    <span class="crash-profile">{profileName}</span>
    <button class="icon-btn" onclick={onClose} title={t("common.close")}><X size={14} /></button>
  </div>
  <p class="crash-notice-summary">{analysis.parsed.diagnosis.summary}</p>
  <div class="crash-notice-actions">
    {#each analysis.parsed.diagnosis.actions.slice(0, 2) as action}
      <span class="crash-action">{action.label}</span>
    {/each}
    <button class="crash-action button ml-auto" onclick={() => onFeedbackOpenChange(true)}>{t("crash_feedback.wrong")}</button>
  </div>
  {#if feedbackOpen}
    <div class="crash-feedback-card">
      <div><p class="crash-feedback-title">{t("crash_feedback.title")}</p><p class="crash-feedback-body">{t("crash_feedback.body")}</p></div>
      <div class="crash-feedback-actions">
        <button class="modal-btn" onclick={() => onCopyReport().catch(console.error)}>{t("crash_feedback.copy")}</button>
        <button class="modal-btn primary" onclick={() => invoke("open_crash_report_issue").catch(console.error)}>{t("crash_feedback.github")}</button>
        <button class="icon-btn" onclick={() => onFeedbackOpenChange(false)} title={t("common.close")}><X size={14} /></button>
      </div>
    </div>
  {/if}
</div>
