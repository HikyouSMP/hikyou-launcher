import { X } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";

import type { CrashAnalysis } from "../types";

export function CrashToast({
  analysis,
  profileName,
  feedbackOpen,
  onFeedbackOpenChange,
  onClose,
  onCopyReport,
}: {
  analysis: CrashAnalysis;
  profileName: string;
  feedbackOpen: boolean;
  onFeedbackOpenChange: (open: boolean) => void;
  onClose: () => void;
  onCopyReport: () => Promise<void>;
}) {
  const { t } = useTranslation();

  return (
    <div className="crash-notice" data-selectable>
      <div className="flex items-center gap-2 min-w-0">
        <span className="crash-chip">{analysis.parsed.diagnosis.category}</span>
        <span className="crash-confidence">
          {Math.round(analysis.parsed.diagnosis.confidence * 100)}%
        </span>
        <span className="crash-profile">{profileName}</span>
        <button className="icon-btn" onClick={onClose} title={t("common.close")}>
          <X size={14} />
        </button>
      </div>
      <p className="crash-notice-summary">
        {analysis.parsed.diagnosis.summary}
      </p>
      <div className="crash-notice-actions">
        {analysis.parsed.diagnosis.actions.slice(0, 2).map((action) => (
          <span key={action.kind} className="crash-action">
            {action.label}
          </span>
        ))}
        <button
          className="crash-action button ml-auto"
          onClick={() => onFeedbackOpenChange(true)}
        >
          {t("crash_feedback.wrong")}
        </button>
      </div>
      {feedbackOpen && (
        <div className="crash-feedback-card">
          <div>
            <p className="crash-feedback-title">
              {t("crash_feedback.title")}
            </p>
            <p className="crash-feedback-body">{t("crash_feedback.body")}</p>
          </div>
          <div className="crash-feedback-actions">
            <button
              className="modal-btn"
              onClick={() => {
                onCopyReport().catch(console.error);
              }}
            >
              {t("crash_feedback.copy")}
            </button>
            <button
              className="modal-btn primary"
              onClick={() => {
                invoke("open_crash_report_issue").catch(console.error);
              }}
            >
              {t("crash_feedback.github")}
            </button>
            <button
              className="icon-btn"
              onClick={() => onFeedbackOpenChange(false)}
              title={t("common.close")}
            >
              <X size={14} />
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
