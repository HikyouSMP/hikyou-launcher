<script lang="ts">
  import { onMount } from "svelte";
  import { Copy, ExternalLink, RefreshCw } from "@lucide/svelte";
  import { createTranslation } from "../lib/i18n.svelte";
  import { useLaunchReport } from "../hooks/useLaunchReport.svelte";
  import ModalBackdrop from "./ModalBackdrop.svelte";

  let { error, onClose }: { error: string; onClose: () => void } = $props();
  const { t } = createTranslation();
  const report = useLaunchReport();
  let dialog: HTMLDialogElement;

  function handleKeys(event: KeyboardEvent) {
    event.stopPropagation();
    if (event.key !== "Tab") return;
    const controls = Array.from(dialog.querySelectorAll<HTMLElement>("button:not(:disabled), input:not(:disabled), textarea:not(:disabled)"));
    const current = controls.indexOf(document.activeElement as HTMLElement);
    const next = event.shiftKey ? current - 1 : current + 1;
    if (current === -1 || next < 0 || next >= controls.length) {
      event.preventDefault();
      controls[event.shiftKey ? controls.length - 1 : 0]?.focus();
    }
  }

  onMount(() => {
    const previousFocus = document.activeElement;
    dialog.showModal();
    void report.prepare(error);
    return () => {
      report.dispose();
      dialog.close();
      if (previousFocus instanceof HTMLElement && previousFocus.isConnected) previousFocus.focus();
    };
  });
</script>

<dialog bind:this={dialog} class="report-dialog" data-focus-scope="modal" data-native-dialog-open data-command-action
  aria-labelledby="launch-report-title" aria-describedby="launch-report-privacy"
  onkeydown={handleKeys}
  oncancel={event => { event.preventDefault(); onClose(); }}>
  <ModalBackdrop />
  <section class="glass-panel modal-card report-card">
    <h2 id="launch-report-title" class="modal-title">{t("launch_report.title")}</h2>
    <p id="launch-report-privacy" class="modal-body">{t("launch_report.privacy")}</p>
    {#if report.preview}
      <label class="preview-label" for="launch-report-preview">{t("launch_report.preview")}</label>
      <textarea id="launch-report-preview" readonly data-selectable value={`${report.preview.title}\n\n${report.preview.body}`} spellcheck="false"></textarea>
      <label class="review-control"><input type="checkbox" bind:checked={report.reviewed} disabled={report.busy} />{t("launch_report.reviewed")}</label>
    {:else if report.busy}
      <p class="modal-body" role="status">{t("launch_report.preparing")}</p>
    {/if}
    {#if report.failure}<p class="modal-body text-danger" role="alert">{t(`launch_report.${report.failure}`)}</p>{/if}
    {#if report.result}<p class="modal-body" role="status">{t(`launch_report.${report.result}`)}</p>{/if}
    <div class="modal-actions report-actions">
      <button type="button" class="modal-btn" onclick={onClose}>{t("common.close")}</button>
      {#if !report.preview}
        <button type="button" class="modal-btn" disabled={report.busy} onclick={() => report.prepare(error)}><RefreshCw size={14} />{t("common.retry")}</button>
      {:else}
        <button type="button" class="modal-btn" disabled={report.busy || !report.reviewed} onclick={report.copy}><Copy size={14} />{t("launch_report.copy")}</button>
        <button type="button" class="modal-btn" disabled={report.busy || !report.reviewed} onclick={report.open}><ExternalLink size={14} />{t("launch_report.open")}</button>
      {/if}
    </div>
  </section>
</dialog>

<style>
  .report-dialog { position: fixed; inset: 0; width: 100%; height: 100%; max-width: none; max-height: none; margin: 0; padding: 12px; border: 0; background: transparent; color: inherit; }
  .report-dialog[open] { display: flex; align-items: center; justify-content: center; }
  .report-dialog::backdrop { background: transparent; }
  .report-card { width: min(100%, 480px); max-height: 100%; overflow-y: auto; }
  .preview-label { display: block; margin-bottom: 6px; font-size: 12px; color: var(--color-t2); }
  textarea { display: block; width: 100%; height: 180px; min-height: 80px; resize: vertical; padding: 10px; border: 1px solid var(--color-b1); border-radius: 6px; background: rgba(0,0,0,.15); color: var(--color-t1); font: 11px/1.6 'JetBrains Mono', monospace; white-space: pre-wrap; overflow-wrap: anywhere; }
  .review-control { display: flex; align-items: flex-start; gap: 8px; margin: 12px 0; font-size: 12px; color: var(--color-t2); }
  .review-control input { flex-shrink: 0; margin-top: 3px; }
  .report-actions { flex-wrap: wrap; }
  .report-actions button { display: inline-flex; align-items: center; justify-content: center; gap: 6px; height: auto; min-height: 32px; white-space: normal; }
  button:disabled { opacity: .45; cursor: default; }
  button:focus-visible, input:focus-visible, textarea:focus-visible { outline: 2px solid var(--color-t2); outline-offset: 2px; }
</style>
