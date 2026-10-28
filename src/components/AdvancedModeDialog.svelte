<script lang="ts">
  import { createTranslation } from "../lib/i18n.svelte";
  import ModalBackdrop from "./ModalBackdrop.svelte";
  import { focusDialogButton, moveDialogFocus } from "./ConfirmDialogKeyboard";
  let { onCancel, onEnable }: { onCancel: () => void; onEnable: () => void } = $props();
  const { t } = createTranslation();
</script>

<div class="adv-overlay absolute inset-0 z-90 flex items-center justify-center">
  <ModalBackdrop />
  <div class="glass-panel modal-card" role="dialog" aria-modal="true" aria-label={t("advanced_confirm.title")} tabindex="-1">
    <p class="modal-title">{t("advanced_confirm.title")}</p><p class="modal-body">{t("advanced_confirm.body")}</p>
    <div class="dlg-btns modal-actions" onkeydown={moveDialogFocus} role="toolbar" tabindex="-1" aria-label={t("advanced_confirm.title")}>
      <button use:focusDialogButton onclick={onCancel} class="modal-btn">{t("common.cancel")}</button>
      <button onclick={onEnable} class="modal-btn primary">{t("common.enable")}</button>
    </div>
  </div>
</div>
