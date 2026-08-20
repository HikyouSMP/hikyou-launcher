import type { RefObject } from "react";
import { Search, Settings, Terminal } from "lucide-react";
import { useTranslation } from "react-i18next";

import { C } from "../theme";
import { Pill } from "./ui";

export function MainHeader({
  inputRef,
  searchValue,
  searchMode,
  debugVisible,
  activeDebug,
  canOpenLogInspector,
  onSearchChange,
  onOpenLogInspector,
  onToggleDebug,
  onOpenSettings,
}: {
  inputRef: RefObject<HTMLInputElement | null>;
  searchValue: string;
  searchMode: "profile" | "modpack";
  debugVisible: boolean;
  activeDebug: boolean;
  canOpenLogInspector: boolean;
  onSearchChange: (value: string) => void;
  onOpenLogInspector: () => void;
  onToggleDebug: () => void;
  onOpenSettings: () => void;
}) {
  const { t } = useTranslation();

  return (
    <div
      data-tauri-drag-region
      className="flex items-center gap-2 px-3 h-13 shrink-0"
      style={{ borderBottom: `1px solid ${C.b1}` }}
    >
      <Search size={16} className="text-t3 shrink-0" />

      {searchMode === "modpack" && (
        <span className="text-[10px] px-1.75 py-px rounded-md font-semibold text-green shrink-0 tracking-[0.03em] bg-green-bg border border-green-bdr">
          MODPACK
        </span>
      )}

      <input
        ref={inputRef}
        value={searchValue}
        onChange={(event) => onSearchChange(event.target.value)}
        placeholder={t("nav.search_placeholder")}
        className="flex-1 bg-transparent border-none outline-none text-[15px] text-t1 font-normal font-[inherit]"
      />

      {canOpenLogInspector && (
        <Pill onClick={onOpenLogInspector} title={t("log.title")}>
          <Terminal size={13} />
        </Pill>
      )}

      {debugVisible && (
        <button
          className="btn-ghost px-2 py-0.75 rounded-md cursor-pointer text-[9px] font-semibold tracking-[0.06em] transition-all duration-120 font-mono"
          title={t("nav.debug_title")}
          onClick={onToggleDebug}
          style={{
            background: activeDebug ? C.greenBg : "none",
            border: activeDebug
              ? `1px solid ${C.greenBdr}`
              : "1px solid transparent",
            color: activeDebug ? C.green : C.t3,
          }}
        >
          DEBUG
        </button>
      )}

      <Pill onClick={onOpenSettings} title={t("common.settings")}>
        <Settings size={13} />
      </Pill>
    </div>
  );
}
