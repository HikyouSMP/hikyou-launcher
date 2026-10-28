<script lang="ts">
  import { createTranslation } from "../lib/i18n.svelte";
  import { css } from "../lib/dom";
  import ModalBackdrop from "./ModalBackdrop.svelte";
  import { focusDialogButton, moveDialogFocus } from "./ConfirmDialogKeyboard";
  let { username, onCancel, onConfirm }: { username?: string; onCancel: () => void; onConfirm: () => void } = $props();
  const { t } = createTranslation();
  function handleKeys(event: KeyboardEvent) { if (event.key === "Escape") { event.stopPropagation(); onCancel(); } }
</script>

<div style={css({ position: "fixed", inset: 0, zIndex: 300, display: "flex", alignItems: "center", justifyContent: "center" })} onclick={onCancel} onkeydown={handleKeys} role="presentation">
  <ModalBackdrop fixed />
  <div onclick={(event) => event.stopPropagation()} onkeydown={handleKeys} class="glass-panel modal-card" style="animation:slideUp .15s cubic-bezier(.16,1,.3,1)" role="dialog" aria-modal="true" aria-label={t("auth.logout_confirm_title")} tabindex="-1">
    <p class="modal-title">{t("auth.logout_confirm_title")}</p>
    <p class="modal-body">{username ? t("auth.logout_confirm_body", { username }) : t("auth.logout_confirm_body_generic")}</p>
    <div class="dlg-btns modal-actions" onkeydown={moveDialogFocus} role="toolbar" tabindex="-1" aria-label={t("auth.logout_confirm_title")}>
      <button use:focusDialogButton onclick={onCancel} class="modal-btn">{t("common.cancel")}</button>
      <button onclick={onConfirm} class="modal-btn danger">{t("auth.logout")}</button>
    </div>
  </div>
</div>
