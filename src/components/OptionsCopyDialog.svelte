<script lang="ts">
  import { createTranslation } from "../lib/i18n.svelte";
  import ModalBackdrop from "./ModalBackdrop.svelte";
  import { focusDialogButton, moveDialogFocus } from "./ConfirmDialogKeyboard";
  let { sourceName, targetName, onCancel, onConfirm }: { sourceName: string; targetName: string; onCancel: () => void; onConfirm: () => void } = $props();
  const { t } = createTranslation();
  function handleKeys(event: KeyboardEvent) {
    event.stopPropagation();
    if (event.key === "Escape") onCancel();
    if (event.key === "Enter") { event.preventDefault(); onConfirm(); }
  }
</script>

<div class="absolute inset-0 z-80 flex items-center justify-center" onclick={onCancel} onkeydown={handleKeys} role="presentation">
  <ModalBackdrop />
  <div onclick={(event) => event.stopPropagation()} onkeydown={handleKeys} class="glass-panel modal-card" role="dialog" aria-modal="true" aria-label={t("profile_options.copy_title")} tabindex="-1">
    <p class="modal-title">{t("profile_options.copy_title")}</p><p class="modal-body">{t("profile_options.copy_body")}</p>
    <div class="options-copy-route" aria-label={`${sourceName} to ${targetName}`}><span class="options-copy-node">{sourceName}</span><span class="options-copy-arrow">→</span><span class="options-copy-node">{targetName}</span></div>
    <div class="dlg-btns modal-actions" onkeydown={moveDialogFocus} role="toolbar" tabindex="-1" aria-label={t("profile_options.copy_title")}>
      <button onclick={onCancel} class="modal-btn">{t("common.cancel")}</button>
      <button use:focusDialogButton onclick={onConfirm} class="modal-btn primary">{t("common.copy")}</button>
    </div>
  </div>
</div>
