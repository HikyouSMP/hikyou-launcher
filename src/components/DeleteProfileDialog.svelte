<script lang="ts">
  import { createTranslation } from "../lib/i18n.svelte";
  import ModalBackdrop from "./ModalBackdrop.svelte";
  import { focusDialogButton, moveDialogFocus } from "./ConfirmDialogKeyboard";
  let { profileName, busy, onCancel, onConfirm, retired = false, error = null, onRestore }: { profileName: string; busy: boolean; onCancel: () => void; onConfirm: () => void; retired?: boolean; error?: string | null; onRestore?: () => void } = $props();
  const { t } = createTranslation();
  function handleKeys(event: KeyboardEvent) {
    event.stopPropagation();
    if (event.key === "Escape" && !busy) onCancel();
    if (event.key === "Enter" && !busy) { event.preventDefault(); if (retired) onRestore?.(); else onConfirm(); }
  }
</script>

<div class="absolute inset-0 z-80 flex items-center justify-center" onclick={() => { if (!busy) onCancel(); }} onkeydown={handleKeys} role="presentation">
  <ModalBackdrop />
  <div onclick={(event) => event.stopPropagation()} onkeydown={handleKeys} class="glass-panel modal-card" role="dialog" aria-modal="true" aria-label={t("profile.delete_title")} tabindex="-1">
    <p class="modal-title">{t("profile.delete_title")}</p>
    <p class="modal-body"><span class="text-t1 font-medium">{profileName}</span> {t(retired ? "profile.retired_confirm" : "profile.delete_confirm")}</p>
    {#if error}<p class="modal-body text-danger" role="alert">{error}</p>{/if}
    <div class="dlg-btns modal-actions" onkeydown={moveDialogFocus} role="toolbar" tabindex="-1" aria-label={t("profile.delete_title")}>
      <button onclick={onCancel} disabled={busy} class="modal-btn">{t(retired ? "common.close" : "common.cancel")}</button>
      <button use:focusDialogButton onclick={() => retired ? onRestore?.() : onConfirm()} disabled={busy} class="modal-btn">{t(retired ? "profile.undo_retirement" : "profile.retire_action")}</button>
    </div>
  </div>
</div>
