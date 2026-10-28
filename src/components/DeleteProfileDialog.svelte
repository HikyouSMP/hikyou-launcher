<script lang="ts">
  import { createTranslation } from "../lib/i18n.svelte";
  import ModalBackdrop from "./ModalBackdrop.svelte";
  import { focusDialogButton, moveDialogFocus } from "./ConfirmDialogKeyboard";
  let { profileName, busy, onCancel, onConfirm }: { profileName: string; busy: boolean; onCancel: () => void; onConfirm: () => void } = $props();
  const { t } = createTranslation();
  function handleKeys(event: KeyboardEvent) {
    event.stopPropagation();
    if (event.key === "Escape") onCancel();
    if (event.key === "Enter" && !busy) { event.preventDefault(); onConfirm(); }
  }
</script>

<div class="absolute inset-0 z-80 flex items-center justify-center" onclick={onCancel} onkeydown={handleKeys} role="presentation">
  <ModalBackdrop />
  <div onclick={(event) => event.stopPropagation()} onkeydown={handleKeys} class="glass-panel modal-card" role="dialog" aria-modal="true" aria-label={t("profile.delete_title")} tabindex="-1">
    <p class="modal-title">{t("profile.delete_title")}</p>
    <p class="modal-body"><span class="text-t1 font-medium">{profileName}</span> {t("profile.delete_confirm")}</p>
    <div class="dlg-btns modal-actions" onkeydown={moveDialogFocus} role="toolbar" tabindex="-1" aria-label={t("profile.delete_title")}>
      <button onclick={onCancel} class="modal-btn">{t("common.cancel")}</button>
      <button use:focusDialogButton onclick={onConfirm} disabled={busy} class="modal-btn danger">{t("common.delete")}</button>
    </div>
  </div>
</div>
