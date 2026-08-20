import { useTranslation } from "react-i18next";

import { C } from "../theme";
import type { StoredAuth } from "../types";

export function MainFooter({
  appVersion,
  savedAuth,
  accountsOpen,
  onVersionClick,
  onToggleAccounts,
  onLogin,
}: {
  appVersion: string;
  savedAuth: StoredAuth | null;
  accountsOpen: boolean;
  onVersionClick: () => void;
  onToggleAccounts: () => void;
  onLogin: () => void;
}) {
  const { t } = useTranslation();

  return (
    <div
      style={{
        borderTop: `1px solid ${C.b2}`,
        display: "flex",
        alignItems: "center",
        padding: "0 8px 0 12px",
        flexShrink: 0,
        height: 36,
        gap: 8,
      }}
    >
      <span
        style={{
          fontSize: 11,
          color: C.t3,
          letterSpacing: "-.01em",
          cursor: "default",
          flexShrink: 0,
        }}
        onClick={onVersionClick}
      >
        Hikyou{appVersion && ` v${appVersion}`}
      </span>

      <div style={{ flex: 1 }} />

      {savedAuth ? (
        <button
        onClick={onToggleAccounts}
        data-account-trigger="true"
        data-open={accountsOpen ? "true" : "false"}
          style={{
            display: "flex",
            alignItems: "center",
            height: 24,
            background: accountsOpen ? C.hover : "transparent",
            border: "none",
            borderRadius: C.r,
            padding: "0 8px",
            color: C.t2,
            fontSize: 11,
            cursor: "pointer",
            letterSpacing: "-.01em",
            transition: "background .16s cubic-bezier(.2,0,0,1)",
            fontFamily: "inherit",
          }}
          onMouseEnter={(event) => {
            if (accountsOpen) return;
            event.currentTarget.style.background = C.hover;
          }}
          onMouseLeave={(event) => {
            if (accountsOpen) return;
            event.currentTarget.style.background = "transparent";
          }}
        >
          <span
            style={{
              maxWidth: 118,
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
              fontWeight: 560,
            }}
          >
            {savedAuth.username ?? "Player"}
          </span>
        </button>
      ) : (
        <button
          onClick={onLogin}
          style={{
            display: "flex",
            alignItems: "center",
            height: 24,
            background: "transparent",
            border: "1px solid transparent",
            borderRadius: C.r,
            padding: "0 8px",
            color: C.t3,
            fontSize: 11,
            cursor: "pointer",
            letterSpacing: "-.01em",
            transition: "color .12s, border-color .12s, background .12s",
            fontFamily: "inherit",
          }}
          onMouseEnter={(event) => {
            event.currentTarget.style.color = C.t2;
            event.currentTarget.style.borderColor = C.b1;
            event.currentTarget.style.background = C.hover;
          }}
          onMouseLeave={(event) => {
            event.currentTarget.style.color = C.t3;
            event.currentTarget.style.borderColor = "transparent";
            event.currentTarget.style.background = "transparent";
          }}
        >
          {t("auth.login_button")}
        </button>
      )}
    </div>
  );
}
