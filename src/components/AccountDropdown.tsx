import { motion, AnimatePresence } from "framer-motion";
import { ChevronUp, User, LogOut, Settings, Plus } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { C } from "../theme";
import type { StoredAuth } from "../types";
import { useAvatarUrl } from "../avatarCache";

interface AccountDropdownProps {
  currentAccount: StoredAuth | null;
  onLogout: () => void;
  onAddAccount: () => void;
  onOpenSettings: () => void;
}

export function AccountDropdown({
  currentAccount,
  onLogout,
  onAddAccount,
  onOpenSettings,
}: AccountDropdownProps) {
  const { t } = useTranslation();
  const [isOpen, setIsOpen] = useState(false);
  const [hoveredItem, setHoveredItem] = useState<string | null>(null);

  const skinName = currentAccount?.uuid ?? currentAccount?.username ?? null;
  const skinUrl = useAvatarUrl(skinName);

  if (!currentAccount) return null;

  const menuItemStyle = (key: string, danger = false): React.CSSProperties => ({
    width: "100%",
    display: "flex",
    alignItems: "center",
    gap: 10,
    padding: "7px 10px",
    borderRadius: C.r,
    background:
      hoveredItem === key
        ? danger
          ? C.dangerBg
          : "rgba(255,255,255,.075)"
        : "transparent",
    border: "none",
    color:
      hoveredItem === key
        ? danger
          ? C.danger
          : C.t1
        : danger
          ? "rgba(160,40,65,.7)"
          : C.t2,
    fontSize: 13,
    cursor: "pointer",
    transition: "background .1s, color .1s",
    textAlign: "left",
  });

  return (
    <div className="fixed bottom-4 right-4 z-[60]">
      <AnimatePresence>
        {isOpen && (
          <>
            {/* Invisible backdrop for closing */}
            <motion.div
              className="fixed inset-0 z-[-1]"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              onClick={() => setIsOpen(false)}
            />

            {/* Menu */}
            <motion.div
              className="account-panel absolute w-[220px] rounded-[8px] overflow-hidden"
              style={{
                bottom: "calc(100% + 8px)",
                right: 0,
              }}
              initial={{ opacity: 0, y: 8, scale: 0.95 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={{ opacity: 0, y: 8, scale: 0.95 }}
              transition={{ duration: 0.15 }}
            >
              {/* Account header */}
              <div className="account-panel-header px-3 py-[10px]">
                <div
                  className="text-[10px] font-semibold text-t3 uppercase tracking-[0.12em] mb-1.5"
                >
                  {t("account_dropdown.active_account")}
                </div>
                <div className="flex items-center gap-[10px]">
                  <div
                    className="account-avatar w-8 h-8 rounded-[6px] flex items-center justify-center text-t3 overflow-hidden shrink-0"
                  >
                    {skinUrl ? (
                      <img
                        src={skinUrl}
                        width={32}
                        height={32}
                        className="block"
                        style={{ imageRendering: "pixelated" }}
                      />
                    ) : (
                      <User size={16} />
                    )}
                  </div>
                  <div className="min-w-0">
                    <div className="text-[13px] font-semibold text-t1 overflow-hidden text-ellipsis whitespace-nowrap">
                      {currentAccount.username ?? t("account_dropdown.unknown_user")}
                    </div>
                    <div className="text-[10px] text-t3">
                      {t("account_dropdown.minecraft_premium")}
                    </div>
                  </div>
                </div>
              </div>

              {/* Menu items */}
              <div className="p-1">
                <button
                  style={menuItemStyle("settings")}
                  onMouseEnter={() => setHoveredItem("settings")}
                  onMouseLeave={() => setHoveredItem(null)}
                  onClick={() => {
                    onOpenSettings();
                    setIsOpen(false);
                  }}
                >
                  <Settings size={14} />
                  {t("account_dropdown.settings")}
                </button>
                <button
                  style={menuItemStyle("add")}
                  onMouseEnter={() => setHoveredItem("add")}
                  onMouseLeave={() => setHoveredItem(null)}
                  onClick={() => {
                    onAddAccount();
                    setIsOpen(false);
                  }}
                >
                  <Plus size={14} />
                  {t("account_dropdown.add_account")}
                </button>

                <div className="account-separator h-px my-1" />

                <button
                  style={menuItemStyle("logout", true)}
                  onMouseEnter={() => setHoveredItem("logout")}
                  onMouseLeave={() => setHoveredItem(null)}
                  onClick={() => {
                    onLogout();
                    setIsOpen(false);
                  }}
                >
                  <LogOut size={14} />
                  {t("account_dropdown.logout")}
                </button>
              </div>
            </motion.div>
          </>
        )}
      </AnimatePresence>

      {/* Trigger button */}
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="account-trigger btn-phys flex items-center gap-2 px-3 py-1.5 rounded-[20px] cursor-pointer transition-colors duration-100"
        style={{
          background: "rgba(46,45,43,.55)",
          backdropFilter: "blur(18px) saturate(150%)",
          WebkitBackdropFilter: "blur(18px) saturate(150%)",
        }}
        onMouseEnter={(e) => {
          (e.currentTarget as HTMLButtonElement).style.background =
            "rgba(255,255,255,.09)";
        }}
        onMouseLeave={(e) => {
          (e.currentTarget as HTMLButtonElement).style.background =
            "rgba(46,45,43,.55)";
        }}
      >
        <div
          className="account-avatar w-6 h-6 rounded-[6px] flex items-center justify-center text-t3 overflow-hidden shrink-0"
        >
          {skinUrl ? (
            <img
              src={skinUrl}
              width={24}
              height={24}
              className="block"
              style={{ imageRendering: "pixelated" }}
            />
          ) : (
            <User size={12} />
          )}
        </div>
        <span className="text-[13px] font-medium text-t1">
          {currentAccount.username ?? t("account_dropdown.unknown_user")}
        </span>
        <ChevronUp
          size={14}
          color={C.t3}
          style={{
            transition: "transform 0.2s",
            transform: isOpen ? "rotate(180deg)" : "rotate(0deg)",
          }}
        />
      </button>
    </div>
  );
}
