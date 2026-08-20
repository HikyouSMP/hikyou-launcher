// ─────────────────────────────────────────────────────────────────────────────
// ProfileConfigPanel — プロファイル構成画面
// ─────────────────────────────────────────────────────────────────────────────
import { useState, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import type { Profile } from "../types";
import { ModalBackdrop } from "./ModalBackdrop";
import { NumberInput } from "./NumberInput";

interface GlobalDefaults {
  memoryMb: number;
  windowW: number;
  windowH: number;
}

interface Props {
  profile: Profile;
  globalDefaults: GlobalDefaults;
  onClose: () => void;
  onSave: (
    id: string,
    changes: {
      name: string;
      memoryMb: number | null;
      windowW: number | null;
      windowH: number | null;
    },
  ) => void;
  onDelete: (id: string) => void;
  onOpenFolder?: () => void;
}

// ── 数値入力 ──────────────────────────────────────────────────────────────────
function NumInput({
  value, onChange, min = 0, max, placeholder, unit, width = 72,
}: {
  value: number; onChange: (v: number) => void;
  min?: number; max?: number; placeholder?: string; unit?: string; width?: number;
}) {
  return (
    <div className="flex items-center gap-1">
      <NumberInput
        value={value}
        onCommit={onChange}
        min={min} max={max} placeholder={placeholder}
        className="pcp-num"
        style={{ width }}
      />
      {unit && <span className="text-[11px] text-t3 shrink-0">{unit}</span>}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
export function ProfileConfigPanel({
  profile,
  globalDefaults,
  onClose,
  onSave,
  onDelete,
  onOpenFolder,
}: Props) {
  const { t } = useTranslation();
  // ── 基本設定 ────────────────────────────────────────────────────────────────
  const [name, setName]               = useState(profile.name);
  const [useGlobalMem, setUseGlobalMem] = useState(profile.memoryMb == null);
  const [memoryMb, setMemoryMb]       = useState(profile.memoryMb ?? globalDefaults.memoryMb);
  const [useGlobalWin, setUseGlobalWin] = useState(profile.windowW == null && profile.windowH == null);
  const [windowW, setWindowW]         = useState(profile.windowW ?? globalDefaults.windowW);
  const [windowH, setWindowH]         = useState(profile.windowH ?? globalDefaults.windowH);
  const panelRef = useRef<HTMLDivElement>(null);

  // フォーカストラップ: Tabキーをパネル内に閉じ込める
  useEffect(() => {
    const panel = panelRef.current;
    if (!panel) return;
    const h = (e: KeyboardEvent) => {
      if (e.key !== "Tab") return;
      const focusable = Array.from(panel.querySelectorAll<HTMLElement>(
        "button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex='-1'])"
      ));
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (e.shiftKey) {
        if (document.activeElement === first) { e.preventDefault(); last.focus(); }
      } else {
        if (document.activeElement === last) { e.preventDefault(); first.focus(); }
      }
    };
    panel.addEventListener("keydown", h);
    return () => panel.removeEventListener("keydown", h);
  }, []);

  useEffect(() => {
    setName(profile.name);
    setUseGlobalMem(profile.memoryMb == null);
    setMemoryMb(profile.memoryMb ?? globalDefaults.memoryMb);
    setUseGlobalWin(profile.windowW == null && profile.windowH == null);
    setWindowW(profile.windowW ?? globalDefaults.windowW);
    setWindowH(profile.windowH ?? globalDefaults.windowH);
  }, [profile.id]);

  const handleSave = () => {
    onSave(profile.id, {
      name,
      memoryMb: useGlobalMem ? null : memoryMb,
      windowW: useGlobalWin ? null : windowW,
      windowH: useGlobalWin ? null : windowH,
    });
  };

  return (
    // position:fixed で viewport 基準に配置 → 親のオーバーフローや位置に依存しない
    <div
      className="fixed inset-0 z-[200] flex items-center justify-center"
      onKeyDown={(e) => {
        if (e.key === "Escape") { e.stopPropagation(); onClose(); }
        if ((e.ctrlKey || e.metaKey) && e.key === "s") { e.preventDefault(); handleSave(); onClose(); }
      }}
    >
      <ModalBackdrop onClick={onClose} />

      <div
        ref={panelRef}
        data-focus-scope="modal"
        onClick={(e) => e.stopPropagation()}
        className="glass-panel modal-card pcp-modal"
        style={{
          width: "min(352px, calc(100vw - 32px))",
          maxHeight: "calc(100vh - 32px)",
          animation: "slideUp .18s cubic-bezier(.16,1,.3,1) forwards",
        }}
      >
        <input
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") { e.preventDefault(); handleSave(); onClose(); }
            if (e.key === "Escape") { e.preventDefault(); onClose(); }
          }}
          className="pcp-title-input"
          aria-label={t("profile_config.name_label")}
        />

        <div className="pcp-fields">
          <section className="pcp-field">
            <div className="pcp-field-head">
              <span>{t("profile_config.memory_label")}</span>
              <strong>{useGlobalMem ? `${globalDefaults.memoryMb} MB` : `${memoryMb} MB`}</strong>
            </div>
            <div className="pcp-field-controls">
              <button
                className={useGlobalMem ? "pcp-switch" : "pcp-switch active"}
                onClick={() => setUseGlobalMem((value) => !value)}
              >
                {useGlobalMem ? "Global" : t("profile_config.custom_badge")}
              </button>
              {!useGlobalMem && (
                <NumInput value={memoryMb} onChange={setMemoryMb} min={512} max={65536} placeholder="2048" unit="MB" width={86} />
              )}
            </div>
          </section>

          <section className="pcp-field">
            <div className="pcp-field-head">
              <span>{t("profile_config.window_size_label")}</span>
              <strong>{useGlobalWin ? `${globalDefaults.windowW}×${globalDefaults.windowH}` : `${windowW}×${windowH}`}</strong>
            </div>
            <div className="pcp-field-controls">
              <button
                className={useGlobalWin ? "pcp-switch" : "pcp-switch active"}
                onClick={() => setUseGlobalWin((value) => !value)}
              >
                {useGlobalWin ? "Global" : t("profile_config.custom_badge")}
              </button>
              {!useGlobalWin && (
                <div className="pcp-inline-control">
                  <NumInput value={windowW} onChange={setWindowW} min={320} max={7680} placeholder="854" unit="W" width={62} />
                  <span className="pcp-x">×</span>
                  <NumInput value={windowH} onChange={setWindowH} min={240} max={4320} placeholder="480" unit="H" width={62} />
                </div>
              )}
            </div>
          </section>
        </div>

        <div className="modal-actions pcp-actions">
          <button
            onClick={() => { onDelete(profile.id); onClose(); }}
            className="modal-btn danger pcp-delete"
          >
            {t("common.delete")}
          </button>
          {onOpenFolder && (
            <button
              onClick={onOpenFolder}
              className="modal-btn pcp-secondary"
            >
              {t("common.folder")}
            </button>
          )}
          <button
            onClick={() => { handleSave(); onClose(); }}
            className="modal-btn primary pcp-save"
          >
            {t("common.save")}
          </button>
        </div>
      </div>
    </div>
  );
}
