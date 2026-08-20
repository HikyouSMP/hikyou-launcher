import { LogOut, UserPlus } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { C } from "../theme";
import type { StoredAuth } from "../types";
import { AccountAvatar } from "./AccountAvatar";

export function AccountPanel({
  accounts,
  savedAuth,
  activeAccountUuid,
  onClose,
  onSwitchAccount,
  onAddAccount,
  onLogoutRequest,
}: {
  accounts: StoredAuth[];
  savedAuth: StoredAuth | null;
  activeAccountUuid: string | null;
  onClose: () => void;
  onSwitchAccount: (account: StoredAuth) => void;
  onAddAccount: () => void;
  onLogoutRequest: (account: StoredAuth) => void;
}) {
  const { t } = useTranslation();
  const [accountListOpen, setAccountListOpen] = useState(false);
  const activeAccount =
    accounts.find((account) =>
      account.uuid != null
        ? account.uuid === activeAccountUuid
        : account.username === savedAuth?.username,
    ) ??
    savedAuth ??
    accounts[0] ??
    null;
  const canSwitchAccounts = accounts.length > 1;
  const isActiveAccount = (account: StoredAuth) =>
    account.uuid != null
      ? account.uuid === activeAccountUuid
      : account.username === savedAuth?.username;
  const orderedAccounts = activeAccount
    ? [
        ...accounts.filter((account) => !isActiveAccount(account)),
        activeAccount,
      ]
    : [];
  const visibleAccounts = accountListOpen
    ? orderedAccounts
    : activeAccount
      ? [activeAccount]
      : [];

  return (
    <>
      <div className="absolute inset-0 z-48" onClick={onClose} />
      <div
        className="absolute bottom-10 right-2 z-50 w-54 overflow-hidden"
        style={{
          background: "rgba(31,30,28,.62)",
          backdropFilter: "blur(26px) saturate(150%)",
          WebkitBackdropFilter: "blur(26px) saturate(150%)",
          borderRadius: 9,
          boxShadow: "0 12px 28px rgba(0,0,0,.28)",
        }}
      >
        {activeAccount && (
          <div className="p-1.5">
            <div
              className="sb overflow-hidden"
              style={{
                display: "flex",
                flexDirection: "column",
                gap: accountListOpen ? 2 : 0,
                transition: "gap .22s cubic-bezier(.2,0,0,1)",
              }}
            >
              {visibleAccounts.map((account, index) => {
                const isActive = isActiveAccount(account);
                const skinId = account.uuid ?? account.username;
                const appearOffset =
                  accountListOpen && !isActive
                    ? Math.max(0, orderedAccounts.length - index - 1) * 4
                    : 0;
                return (
                  <div
                    key={account.uuid ?? account.username}
                    style={{
                      maxHeight: 40,
                      opacity: 1,
                      transform: accountListOpen
                        ? `translateY(${appearOffset}px) scale(1)`
                        : "translateY(0) scale(1)",
                      transformOrigin: "bottom center",
                      pointerEvents: "auto",
                      overflow: "hidden",
                      animation:
                        accountListOpen && !isActive
                          ? "accountOptionIn .24s cubic-bezier(.2,0,0,1) both"
                          : undefined,
                      animationDelay:
                        accountListOpen && !isActive
                          ? `${Math.max(0, orderedAccounts.length - index - 1) * 18}ms`
                          : undefined,
                      transition:
                        "opacity .2s cubic-bezier(.2,0,0,1), transform .28s cubic-bezier(.2,0,0,1)",
                    }}
                  >
                    <div
                      className="group flex items-center gap-1 rounded-[7px]"
                      style={{
                        background: isActive
                          ? accountListOpen
                            ? "rgba(61,168,92,.14)"
                            : "rgba(255,255,255,.06)"
                          : "transparent",
                        transition:
                          "background .18s cubic-bezier(.2,0,0,1), color .18s cubic-bezier(.2,0,0,1)",
                      }}
                      onMouseEnter={(event) => {
                        if (!isActive || !accountListOpen) {
                          event.currentTarget.style.background =
                            "rgba(255,255,255,.08)";
                        }
                      }}
                      onMouseLeave={(event) => {
                        event.currentTarget.style.background = isActive
                          ? accountListOpen
                            ? "rgba(61,168,92,.14)"
                            : "rgba(255,255,255,.06)"
                          : "transparent";
                      }}
                    >
                      <button
                        onClick={() => {
                          if (isActive) {
                            if (canSwitchAccounts) {
                              setAccountListOpen((open) => !open);
                            }
                          } else {
                            onSwitchAccount(account);
                          }
                        }}
                        className="min-w-0 flex-1 flex items-center gap-2 px-2 py-2 text-left"
                        style={{
                          background: "transparent",
                          border: "none",
                          cursor:
                            isActive && !canSwitchAccounts ? "default" : "pointer",
                        }}
                      >
                        <div className="w-6 h-6 rounded-md shrink-0 flex items-center justify-center text-[11px] font-medium text-t1 overflow-hidden bg-transparent">
                          <AccountAvatar
                            skinId={skinId}
                            size={24}
                            fallback={(account.username ?? "?")[0].toUpperCase()}
                          />
                        </div>
                        <div className="flex-1 min-w-0">
                          <p
                            className="text-xs overflow-hidden text-ellipsis whitespace-nowrap"
                            style={{
                              color:
                                isActive && accountListOpen ? C.green : C.t1,
                              fontWeight: isActive ? 560 : 500,
                            }}
                          >
                            {account.username ?? "Unknown"}
                          </p>
                        </div>
                      </button>
                      {(accountListOpen || !canSwitchAccounts) && (
                        <button
                          onClick={(event) => {
                            event.stopPropagation();
                            onLogoutRequest(account);
                          }}
                          title={t("auth.logout")}
                          className="mr-1 flex h-7 w-7 items-center justify-center rounded-md text-t3"
                          style={{
                            background: "transparent",
                            border: "none",
                            cursor: "pointer",
                            transition:
                              "background .16s cubic-bezier(.2,0,0,1), color .16s cubic-bezier(.2,0,0,1)",
                          }}
                          onMouseEnter={(event) => {
                            event.currentTarget.style.background = C.dangerBg;
                            event.currentTarget.style.color = C.danger;
                          }}
                          onMouseLeave={(event) => {
                            event.currentTarget.style.background = "transparent";
                            event.currentTarget.style.color = C.t3;
                          }}
                        >
                          <LogOut size={11} />
                        </button>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {!savedAuth && accounts.length === 0 && (
          <div className="p-3">
            <button
              className="btn-phys w-full py-2.75 rounded-md cursor-pointer text-[13px] font-[inherit]"
              onClick={onAddAccount}
              style={{
                background: "rgba(255,255,255,.07)",
                border: `1px solid ${C.b1}`,
                color: C.t1,
              }}
            >
              {t("auth.login_microsoft")}
            </button>
          </div>
        )}

        <div className="px-1.5 pb-1.5 pt-0.5" style={{ borderTop: "none" }}>
          <button
            className="btn-ghost btn-phys w-full flex items-center gap-1.75 px-2.5 py-2 rounded-md text-t2 text-xs cursor-pointer transition-all duration-100 text-left"
            onClick={onAddAccount}
            onMouseEnter={(event) => {
              event.currentTarget.style.background = C.hover;
              event.currentTarget.style.color = C.t1;
            }}
            onMouseLeave={(event) => {
              event.currentTarget.style.background = "transparent";
              event.currentTarget.style.color = C.t2;
            }}
          >
            <UserPlus size={11} /> {t("auth.add_account")}
          </button>
        </div>
      </div>
    </>
  );
}
