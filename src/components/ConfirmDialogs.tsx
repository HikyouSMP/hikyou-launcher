import { useTranslation } from "react-i18next";

import { ModalBackdrop } from "./ModalBackdrop";

function moveDialogFocus(event: React.KeyboardEvent<HTMLElement>) {
  if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
  const buttons = Array.from(
    event.currentTarget.querySelectorAll<HTMLElement>("button"),
  );
  const current = buttons.indexOf(document.activeElement as HTMLElement);
  if (event.key === "ArrowLeft") {
    buttons[current <= 0 ? buttons.length - 1 : current - 1]?.focus();
  } else {
    buttons[current >= buttons.length - 1 ? 0 : current + 1]?.focus();
  }
}

export function AdvancedModeDialog({
  onCancel,
  onEnable,
}: {
  onCancel: () => void;
  onEnable: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="adv-overlay absolute inset-0 z-90 flex items-center justify-center">
      <ModalBackdrop />
      <div className="glass-panel modal-card">
        <p className="modal-title">
          {t("advanced_confirm.title")}
        </p>
        <p className="modal-body">
          {t("advanced_confirm.body")}
        </p>
        <div className="dlg-btns modal-actions" onKeyDown={moveDialogFocus}>
          <button autoFocus onClick={onCancel} className="modal-btn">
            {t("common.cancel")}
          </button>
          <button onClick={onEnable} className="modal-btn primary">
            {t("common.enable")}
          </button>
        </div>
      </div>
    </div>
  );
}

export function DeleteProfileDialog({
  profileName,
  busy,
  onCancel,
  onConfirm,
}: {
  profileName: string;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div
      className="absolute inset-0 z-80 flex items-center justify-center"
      onClick={onCancel}
      onKeyDown={(event) => {
        event.stopPropagation();
        if (event.key === "Escape") onCancel();
        if (event.key === "Enter" && !busy) {
          event.preventDefault();
          onConfirm();
        }
      }}
    >
      <ModalBackdrop />
      <div
        onClick={(event) => event.stopPropagation()}
        className="glass-panel modal-card"
      >
        <p className="modal-title">
          {t("profile.delete_title")}
        </p>
        <p className="modal-body">
          <span className="text-t1 font-medium">{profileName}</span>{" "}
          {t("profile.delete_confirm")}
        </p>
        <div className="dlg-btns modal-actions" onKeyDown={moveDialogFocus}>
          <button onClick={onCancel} className="modal-btn">
            {t("common.cancel")}
          </button>
          <button
            autoFocus
            onClick={onConfirm}
            disabled={busy}
            className="modal-btn danger"
          >
            {t("common.delete")}
          </button>
        </div>
      </div>
    </div>
  );
}

export function LogoutConfirmDialog({
  username,
  onCancel,
  onConfirm,
}: {
  username?: string;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 300,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
      onClick={onCancel}
      onKeyDown={(event) => {
        if (event.key === "Escape") onCancel();
      }}
    >
      <ModalBackdrop fixed />
      <div
        onClick={(event) => event.stopPropagation()}
        className="glass-panel modal-card"
        style={{ animation: "slideUp .15s cubic-bezier(.16,1,.3,1)" }}
      >
        <p className="modal-title">
          {t("auth.logout_confirm_title")}
        </p>
        <p className="modal-body">
          {username
            ? t("auth.logout_confirm_body", { username })
            : t("auth.logout_confirm_body_generic")}
        </p>
        <div className="dlg-btns modal-actions" onKeyDown={moveDialogFocus}>
          <button autoFocus onClick={onCancel} className="modal-btn">
            {t("common.cancel")}
          </button>
          <button onClick={onConfirm} className="modal-btn danger">
            {t("auth.logout")}
          </button>
        </div>
      </div>
    </div>
  );
}

export function OptionsCopyDialog({
  sourceName,
  targetName,
  onCancel,
  onConfirm,
}: {
  sourceName: string;
  targetName: string;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div
      className="absolute inset-0 z-80 flex items-center justify-center"
      onClick={onCancel}
      onKeyDown={(event) => {
        event.stopPropagation();
        if (event.key === "Escape") onCancel();
        if (event.key === "Enter") {
          event.preventDefault();
          onConfirm();
        }
      }}
    >
      <ModalBackdrop />
      <div
        onClick={(event) => event.stopPropagation()}
        className="glass-panel modal-card"
      >
        <p className="modal-title">{t("profile_options.copy_title")}</p>
        <p className="modal-body">
          {t("profile_options.copy_body")}
        </p>
        <div className="options-copy-route" aria-label={`${sourceName} to ${targetName}`}>
          <span className="options-copy-node">{sourceName}</span>
          <span className="options-copy-arrow">→</span>
          <span className="options-copy-node">{targetName}</span>
        </div>
        <div className="dlg-btns modal-actions" onKeyDown={moveDialogFocus}>
          <button onClick={onCancel} className="modal-btn">
            {t("common.cancel")}
          </button>
          <button autoFocus onClick={onConfirm} className="modal-btn primary">
            {t("common.copy")}
          </button>
        </div>
      </div>
    </div>
  );
}
