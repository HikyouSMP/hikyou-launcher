// ─────────────────────────────────────────────────────────────────────────────
// ModListView — 共有Modリストコンポーネント
// ModsPanel / RecModsPanel 共通の UI とキーボードナビゲーション
//
// ホバー (マウス) とキーボードフォーカスは独立した状態として管理する。
//   kbIdx   : 矢印キーで操作するキーボードフォーカス (C.hover)
//   hoverIdx: マウスカーソルによるホバー (C.hoverLight)
// ─────────────────────────────────────────────────────────────────────────────

import React, { useState, useRef, useEffect, useCallback } from "react";
import { ArrowLeft, Search } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { C } from "../theme";
import type { ModSearchResult } from "../types";

const DEBOUNCE_MS = 350;

// ── ModIcon (共有) ────────────────────────────────────────────────────────────
// url が渡されると fetch_cached_icon でディスクキャッシュ済み data URL を取得する。
// キャッシュがある場合はネットワーク不要。失敗時は元 URL にフォールバック。
export function ModIcon({
  url, name, size = 28, faded = false,
}: { url?: string | null; name: string; size?: number; faded?: boolean }) {
  const [src, setSrc] = useState<string | null>(url ?? null);
  const [err, setErr] = useState(false);

  useEffect(() => {
    if (!url) { setSrc(null); return; }
    setSrc(url); // まず直接 URL で表示（フラッシュを防ぐ）
    setErr(false);
    invoke<string>("fetch_cached_icon", { url })
      .then((dataUrl) => setSrc(dataUrl))
      .catch(() => { /* 元 URL のまま継続 */ });
  }, [url]);

  const letter = (name || "?")[0].toUpperCase();
  const baseStyle: React.CSSProperties = {
    width: size, height: size, borderRadius: 6, flexShrink: 0,
    display: "block", opacity: faded ? 0.35 : 1, transition: "opacity .2s",
  };
  if (src && !err) {
    return <img src={src} alt="" onError={() => setErr(true)} style={{ ...baseStyle, objectFit: "cover" }} />;
  }
  return (
    <div style={{ ...baseStyle, background: C.surface, display: "flex", alignItems: "center", justifyContent: "center", fontSize: size * 0.45, color: C.t3, fontWeight: 600, userSelect: "none" }}>
      {letter}
    </div>
  );
}

// ── 型定義 ────────────────────────────────────────────────────────────────────

export interface MLEntry {
  id: string;
  name: string;
  icon?: string | null;
  enabled: boolean;
  subtitle?: string;
  badges?: React.ReactNode;
}

export interface ModListViewProps {
  onClose: () => void;
  searchPlaceholder: string;
  headerRight?: React.ReactNode;
  entries: MLEntry[];
  loading: boolean;
  emptyNode?: React.ReactNode;
  errorNode?: React.ReactNode;
  onEntryClick?: (id: string) => void;
  onEntryDelete: (id: string) => void;
  loader: string;
  mcVersion: string;
  searchState: (projectId: string) => "idle" | "loading" | "done";
  onInstall: (mod: ModSearchResult) => Promise<void>;
  onSearchResultDelete?: (mod: ModSearchResult) => void;
  renderSearchMeta?: (mod: ModSearchResult) => React.ReactNode;
}

// ─────────────────────────────────────────────────────────────────────────────
export function ModListView({
  onClose,
  searchPlaceholder,
  headerRight,
  entries,
  loading,
  emptyNode,
  errorNode,
  onEntryClick,
  onEntryDelete,
  loader,
  mcVersion,
  searchState,
  onInstall,
  onSearchResultDelete,
  renderSearchMeta,
}: ModListViewProps) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<ModSearchResult[]>([]);
  const [searching, setSearching] = useState(false);
  const [searchErr, setSearchErr] = useState<string | null>(null);

  // ── キーボードフォーカス (矢印キーのみが変更する) ──────────────────────────
  const [kbIdx, setKbIdx] = useState(0);
  const kbIdxRef = useRef(0);

  // ── マウスホバー (マウス移動のみが変更する) ────────────────────────────────
  const [hoverIdx, setHoverIdx] = useState<number | null>(null);
  const hoverIdxRef = useRef<number | null>(null);

  const [pendingDelete, setPendingDelete] = useState<string | null>(null);
  const [pendingDeleteSearch, setPendingDeleteSearch] = useState<string | null>(null);

  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const scrollTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const itemRefs = useRef<Map<number, HTMLElement>>(new Map());

  // スクロール中はホバーハイライトを無効化してポインターイベントを止める
  const handleListScroll = () => {
    setHoverIdx(null);
    if (listRef.current) listRef.current.style.pointerEvents = "none";
    if (scrollTimerRef.current) clearTimeout(scrollTimerRef.current);
    scrollTimerRef.current = setTimeout(() => {
      if (listRef.current) listRef.current.style.pointerEvents = "";
    }, 150);
  };

  const isSearchMode = query.trim().length > 0;

  // ── 検索 ──────────────────────────────────────────────────────────────────
  const runSearch = useCallback((q: string) => {
    if (!q.trim()) { setResults([]); setKbIdx(0); kbIdxRef.current = 0; return; }
    setSearching(true);
    setSearchErr(null);
    invoke<ModSearchResult[]>("search_modrinth", { query: q.trim(), loader, mcVersion })
      .then((r) => { setResults(r); setKbIdx(0); kbIdxRef.current = 0; })
      .catch((e) => setSearchErr(String(e)))
      .finally(() => setSearching(false));
  }, [loader, mcVersion]);

  const handleQueryChange = (v: string) => {
    setQuery(v);
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => runSearch(v), DEBOUNCE_MS);
  };

  // ── キーボードナビゲーション ──────────────────────────────────────────────
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      // Escape
      if (e.key === "Escape") {
        e.stopPropagation();
        if (pendingDelete || pendingDeleteSearch) {
          setPendingDelete(null); setPendingDeleteSearch(null); return;
        }
        if (query.trim()) {
          setQuery(""); setResults([]); setKbIdx(0); kbIdxRef.current = 0; return;
        }
        onClose();
        return;
      }

      const count = isSearchMode ? results.length : entries.length;

      // Alt+Arrow (全 Alt コンボで OS メニュー起動を防ぐ)
      if (e.altKey) {
        e.preventDefault();
        if (e.key === "ArrowRight") {
          if (pendingDelete || pendingDeleteSearch) {
            setPendingDelete(null); setPendingDeleteSearch(null); return;
          }
          const idx = kbIdxRef.current;
          if (isSearchMode && idx >= 0 && idx < results.length) {
            const mod = results[idx];
            if (searchState(mod.project_id) === "idle") onInstall(mod);
          }
          return;
        }
        if (e.key === "ArrowLeft") {
          e.preventDefault();
          const idx = kbIdxRef.current;
          if (isSearchMode) {
            if (idx >= 0 && idx < results.length) {
              const mod = results[idx];
              if (searchState(mod.project_id) === "done") {
                if (pendingDeleteSearch === mod.project_id) {
                  if (onSearchResultDelete) onSearchResultDelete(mod);
                  else onEntryDelete(mod.project_id);
                  setPendingDeleteSearch(null);
                } else {
                  setPendingDeleteSearch(mod.project_id);
                }
              }
            }
          } else {
            if (idx >= 0 && idx < entries.length) {
              const entry = entries[idx];
              if (pendingDelete === entry.id) {
                onEntryDelete(entry.id); setPendingDelete(null);
              } else {
                setPendingDelete(entry.id);
              }
            }
          }
          return;
        }
        return;
      }

      // Arrow Up/Down — キーボードフォーカスのみを変更 (マウスホバーには触れない)
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        if (count === 0) return;
        e.preventDefault();
        if (inputRef.current) {
          const pos = inputRef.current.selectionEnd ?? inputRef.current.value.length;
          inputRef.current.setSelectionRange(pos, pos);
        }
        const cur = kbIdxRef.current;
        let next = cur;
        if (e.key === "ArrowDown") {
          if (cur >= count - 1) next = e.repeat ? count - 1 : 0;
          else next = cur < 0 ? 0 : cur + 1;
        } else {
          if (cur <= 0) {
            if (cur < 0) next = count - 1;
            else next = e.repeat ? 0 : count - 1;
          } else {
            next = cur - 1;
          }
        }
        kbIdxRef.current = next;
        setKbIdx(next);
        setHoverIdx(null);
        return;
      }

      if (kbIdxRef.current < 0 || kbIdxRef.current >= count) return;

      // Enter
      if (e.key === "Enter") {
        e.preventDefault();
        const idx = kbIdxRef.current;
        if (isSearchMode) {
          if (idx < results.length) {
            const mod = results[idx];
            const state = searchState(mod.project_id);
            if (state === "done" && pendingDeleteSearch === mod.project_id) {
              if (onSearchResultDelete) onSearchResultDelete(mod);
              else onEntryDelete(mod.project_id);
              setPendingDeleteSearch(null);
            } else if (state === "idle") {
              onInstall(mod);
            }
          }
        } else {
          if (idx < entries.length) {
            const entry = entries[idx];
            if (pendingDelete === entry.id) {
              onEntryDelete(entry.id); setPendingDelete(null);
            } else if (onEntryClick) {
              onEntryClick(entry.id);
            }
          }
        }
        return;
      }

      // Delete/Backspace は検索バーと競合するため削除ショートカットから除外。
      // 削除は Alt+← で行う。
    };
    window.addEventListener("keydown", h, true);
    return () => window.removeEventListener("keydown", h, true);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, isSearchMode, results, entries, pendingDelete, pendingDeleteSearch, onClose, searchState, onInstall, onEntryDelete, onEntryClick, onSearchResultDelete]);

  // キーボードフォーカス変更で削除確認をキャンセル
  useEffect(() => { setPendingDelete(null); setPendingDeleteSearch(null); }, [kbIdx]);

  // キーボードフォーカス項目をスクロールイン
  useEffect(() => {
    if (kbIdx >= 0) itemRefs.current.get(kbIdx)?.scrollIntoView({ block: "nearest" });
  }, [kbIdx]);

  useEffect(() => { kbIdxRef.current = kbIdx; }, [kbIdx]);
  useEffect(() => { hoverIdxRef.current = hoverIdx; }, [hoverIdx]);

  // 検索結果が変わったら先頭へ
  useEffect(() => {
    setKbIdx(0); kbIdxRef.current = 0;
    setHoverIdx(null);
    requestAnimationFrame(() => itemRefs.current.get(0)?.scrollIntoView({ block: "nearest" }));
  }, [results]); // eslint-disable-line

  // 初期フォーカス
  useEffect(() => { setTimeout(() => inputRef.current?.focus(), 60); }, []);

  // ── 描画 ──────────────────────────────────────────────────────────────────
  return (
    <div className="flex-1 flex flex-col overflow-hidden">

      {/* ヘッダー */}
      <div
        className="flex items-center px-[10px] h-[52px] shrink-0 gap-2 border-b border-b1"
      >
        <button
          onClick={onClose}
          className="flex items-center px-[6px] py-1 rounded-[6px] cursor-pointer transition-colors duration-[120ms] bg-transparent border-0 text-t3"
          onMouseEnter={(e) => (e.currentTarget.style.color = C.t1)}
          onMouseLeave={(e) => (e.currentTarget.style.color = C.t3)}
        >
          <ArrowLeft size={14} />
        </button>
        <Search size={14} className="text-t3 shrink-0" />
        <input
          ref={inputRef}
          value={query}
          onChange={(e) => handleQueryChange(e.target.value)}
          placeholder={searchPlaceholder}
          className="flex-1 bg-transparent border-none outline-none text-sm text-t1 font-light font-[inherit]"
        />
        {query && (
          <button
            onClick={() => { setQuery(""); setResults([]); setKbIdx(0); kbIdxRef.current = 0; inputRef.current?.focus(); }}
            className="cursor-pointer text-t3 text-base leading-none px-[2px] bg-transparent border-0"
          >×</button>
        )}
        {headerRight}
      </div>

      {/* エラー表示 */}
      {errorNode}
      {searchErr && <div className="px-4 py-[6px] text-[11px] text-danger">{searchErr}</div>}

      {/* リスト */}
      <div ref={listRef} className="sb flex-1 overflow-y-auto py-[6px]" onScroll={handleListScroll}>

        {/* 検索モード */}
        {isSearchMode && searching && (
          <div className="py-8 text-center text-xs text-t3">{t("mods.searching")}</div>
        )}
        {isSearchMode && !searching && results.length === 0 && query.trim() && (
          <div className="py-8 px-4 text-center text-xs text-t3">
            {t("mods.no_search_result", { query })}
          </div>
        )}
        {isSearchMode && !searching && results.map((mod, idx) => {
          const kbFocused = kbIdx === idx;
          const hovered = hoverIdx === idx;
          const state = searchState(mod.project_id);
          const isPendingDel = pendingDeleteSearch === mod.project_id && state === "done";
          const isActive = kbFocused || hovered;
          const bg = isPendingDel && isActive ? C.dangerBg
            : kbFocused ? C.hover
            : hovered ? C.hoverLight
            : "transparent";
          return (
            <div
              key={mod.project_id}
              ref={(el) => { if (el) itemRefs.current.set(idx, el); else itemRefs.current.delete(idx); }}
              onMouseEnter={() => setHoverIdx(idx)}
              onMouseMove={() => { if (hoverIdxRef.current !== idx) setHoverIdx(idx); }}
              onMouseLeave={() => setHoverIdx(null)}
              className="flex items-center px-3 gap-[10px] rounded-[6px] mx-[6px] mb-[1px] h-11 box-border overflow-hidden"
              style={{ background: bg, scrollMarginTop: 8, scrollMarginBottom: 8, transition: "none" }}
            >
              <ModIcon url={mod.icon_url} name={mod.title} size={28} />
              <div className="flex-1 min-w-0">
                {isPendingDel ? (
                  <div className="text-[13px] text-danger">{t("mods.delete_confirm")}</div>
                ) : (
                  <>
                    <div className="text-[13px] text-t1 whitespace-nowrap overflow-hidden text-ellipsis">{mod.title}</div>
                    {renderSearchMeta ? renderSearchMeta(mod) : (
                      <div className="text-[10px] text-t3 mt-[1px]">↓ {fmtDl(mod.downloads)}</div>
                    )}
                  </>
                )}
              </div>
              {state !== "done" ? (
                <button
                  onClick={() => state === "idle" && onInstall(mod)}
                  disabled={state === "loading"}
                  title={t("mods.install_add")}
                  className="rounded-[6px] w-7 h-7 flex items-center justify-center shrink-0"
                  style={{
                    background: isActive ? C.greenBg : "transparent",
                    border: "none",
                    color: isActive ? C.green : C.t3,
                    cursor: state === "loading" ? "default" : "pointer",
                    opacity: state === "loading" ? 0.5 : 1,
                    transition: "none",
                  }}
                >
                  {state === "loading" ? "…" : <DownloadIcon />}
                </button>
              ) : (
                <button
                  onClick={() => {
                    if (isPendingDel) {
                      if (onSearchResultDelete) onSearchResultDelete(mod);
                      else onEntryDelete(mod.project_id);
                      setPendingDeleteSearch(null);
                    } else {
                      setPendingDeleteSearch(mod.project_id);
                    }
                  }}
                  title={t("mods.install_delete")}
                  className="rounded-[6px] w-7 h-7 flex items-center justify-center shrink-0 cursor-pointer text-[13px] px-1 py-[2px] leading-none"
                  style={{
                    background: "none",
                    border: "none",
                    color: isPendingDel ? C.danger : (isActive ? C.t3 : "transparent"),
                    transition: "color .1s",
                    pointerEvents: isActive ? "auto" : "none",
                  }}
                  onMouseEnter={(e) => (e.currentTarget.style.color = C.danger)}
                  onMouseLeave={(e) => (e.currentTarget.style.color = isPendingDel ? C.danger : C.t3)}
                >✕</button>
              )}
            </div>
          );
        })}

        {/* インストール済みモード */}
        {!isSearchMode && loading && (
          <div className="py-8 text-center text-xs text-t3">{t("mods.loading")}</div>
        )}
        {!isSearchMode && !loading && entries.length === 0 && (
          emptyNode ?? (
            <div className="py-8 px-4 text-center text-xs text-t3 leading-[1.6]">
              {t("mods.empty")}<br /><span className="text-[10px]">{t("mods.empty_hint")}</span>
            </div>
          )
        )}
        {!isSearchMode && !loading && entries.map((entry, idx) => {
          const kbFocused = kbIdx === idx;
          const hovered = hoverIdx === idx;
          const isPendingDel = pendingDelete === entry.id;
          const isActive = kbFocused || hovered;
          const bg = isPendingDel && isActive ? C.dangerBg
            : kbFocused ? C.hover
            : hovered ? C.hoverLight
            : "transparent";
          return (
            <div
              key={entry.id}
              ref={(el) => { if (el) itemRefs.current.set(idx, el); else itemRefs.current.delete(idx); }}
              onClick={() => onEntryClick?.(entry.id)}
              onMouseEnter={() => setHoverIdx(idx)}
              onMouseMove={() => { if (hoverIdxRef.current !== idx) setHoverIdx(idx); }}
              onMouseLeave={() => setHoverIdx(null)}
              className="flex items-center px-3 gap-[10px] rounded-[6px] mx-[6px] mb-[1px] h-10 box-border overflow-hidden"
              style={{ background: bg, scrollMarginTop: 8, scrollMarginBottom: 8, transition: "none", cursor: onEntryClick ? "pointer" : "default" }}
            >
              <ModIcon url={entry.icon} name={entry.name} size={28} faded={!entry.enabled} />
              <div className="flex-1 min-w-0 transition-opacity duration-200" style={{ opacity: entry.enabled ? 1 : 0.4 }}>
                {isPendingDel ? (
                  <div className="text-[13px] text-danger whitespace-nowrap overflow-hidden text-ellipsis">
                    {t("mods.delete_confirm")}
                  </div>
                ) : (
                  <>
                    <div className="flex items-center gap-1">
                      <span className="text-[13px] text-t1 whitespace-nowrap overflow-hidden text-ellipsis">{entry.name}</span>
                      {entry.badges}
                    </div>
                    {entry.subtitle && (
                      <div className="text-[9px] text-t3 mt-[1px] whitespace-nowrap overflow-hidden text-ellipsis" style={{ fontFamily: "'JetBrains Mono','SF Mono',monospace" }}>{entry.subtitle}</div>
                    )}
                  </>
                )}
              </div>
              <button
                onClick={(e) => { e.stopPropagation(); isPendingDel ? (onEntryDelete(entry.id), setPendingDelete(null)) : setPendingDelete(entry.id); }}
                title={t("mods.install_delete")}
                className="rounded-[6px] cursor-pointer text-[13px] px-1 py-[2px] leading-none"
                style={{
                  background: "none",
                  border: "none",
                  color: isPendingDel ? C.danger : (isActive ? C.t3 : "transparent"),
                  transition: "color .1s, opacity .1s",
                  pointerEvents: isActive ? "auto" : "none",
                }}
                onMouseEnter={(e) => (e.currentTarget.style.color = C.danger)}
                onMouseLeave={(e) => (e.currentTarget.style.color = isPendingDel ? C.danger : C.t3)}
              >✕</button>
            </div>
          );
        })}

        {/* キーボードショートカットヒント */}
        {!isSearchMode && entries.length > 0 && (
          <div className="px-3 pt-2 pb-1 text-[9px] text-t3 text-center opacity-60">
            {t("mods.kb_hint_installed")}
          </div>
        )}
        {isSearchMode && results.length > 0 && (
          <div className="px-3 pt-2 pb-1 text-[9px] text-t3 text-center opacity-60">
            {t("mods.kb_hint_search")}
          </div>
        )}
      </div>
    </div>
  );
}

// ── ヘルパー ──────────────────────────────────────────────────────────────────
function fmtDl(n: number) {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1000).toFixed(0)}K`;
  return String(n);
}

function DownloadIcon() {
  return (
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
      <polyline points="7 10 12 15 17 10" />
      <line x1="12" y1="15" x2="12" y2="3" />
    </svg>
  );
}
