import { useTranslation } from "react-i18next";

import { C } from "../theme";

export function GameErrorBanner({
  error,
  onClose,
}: {
  error: string;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const message = (error.match(/message:\s*([^,}]+)/i)?.[1] ?? error)
    .replace(/^Error:\s*/i, "")
    .trim();

  return (
    <div
      style={{
        margin: "4px 8px",
        borderRadius: C.r,
        background: C.dangerBg,
        border: `1px solid ${C.dangerBdr}`,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          padding: "8px 12px 4px",
        }}
      >
        <span
          style={{
            color: "rgba(248,113,113,.9)",
            fontSize: 11,
            fontWeight: 700,
            letterSpacing: "0.04em",
          }}
        >
          {t("game_error.title")}
        </span>
        <button
          onClick={onClose}
          style={{
            background: "none",
            border: "none",
            color: "rgba(248,113,113,.5)",
            cursor: "pointer",
            fontSize: 16,
            lineHeight: 1,
            userSelect: "none",
          }}
        >
          ×
        </button>
      </div>
      <p
        style={{
          padding: "0 12px 10px",
          color: "rgba(248,113,113,.75)",
          fontSize: 10,
          lineHeight: 1.65,
          wordBreak: "break-all",
          fontFamily: "'JetBrains Mono','JetBrains Mono','SF Mono','Fira Code',monospace",
        }}
      >
        {message}
      </p>
    </div>
  );
}
