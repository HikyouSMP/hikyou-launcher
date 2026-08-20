// ─────────────────────────────────────────────────────────────────────────────
// Hikyou UI — 共通コンポーネント
// ─────────────────────────────────────────────────────────────────────────────

import React from "react";
import { Copy, FolderOpen } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { C } from "../theme";

// ── Pill ─────────────────────────────────────────────────────────────────────

/** ヘッダー用アイコンボタン */
export function Pill({
  children,
  onClick,
  title,
}: {
  children: React.ReactNode;
  onClick: () => void;
  title?: string;
}) {
  return (
    <button
      title={title}
      onClick={onClick}
      className="btn-phys w-8 h-8 flex items-center justify-center rounded-[6px] shrink-0 cursor-pointer transition-[background,color] duration-100 bg-transparent border-0 text-t3"
      onMouseEnter={(e) => {
        e.currentTarget.style.background = C.hover;
        e.currentTarget.style.color = C.t2;
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.background = "transparent";
        e.currentTarget.style.color = C.t3;
      }}
    >
      {children}
    </button>
  );
}

// ── SGroup ───────────────────────────────────────────────────────────────────

/** 設定グループ */
export function SGroup({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <p
        className="text-[11px] font-medium tracking-[.06em] uppercase text-t2 mb-2 pl-[2px]"
      >
        {label}
      </p>
      <div
        className="rounded-[6px] bg-[rgba(255,255,255,.025)]"
      >
        {children}
      </div>
    </div>
  );
}

// ── SRow ─────────────────────────────────────────────────────────────────────

/** 設定行 */
export function SRow({
  label,
  sub,
  children,
  last,
}: {
  label: string;
  sub?: string;
  children: React.ReactNode;
  last?: boolean;
}) {
  const [hov, setHov] = React.useState(false);
  const subBoxRef = React.useRef<HTMLSpanElement | null>(null);
  const subTextRef = React.useRef<HTMLSpanElement | null>(null);
  const marqueeTimerRef = React.useRef<ReturnType<typeof window.setTimeout> | null>(null);
  const [marqueeActive, setMarqueeActive] = React.useState(false);
  const [marquee, setMarquee] = React.useState({
    overflowPx: 0,
    durationSec: 0,
  });

  React.useLayoutEffect(() => {
    if (!sub || !hov || !subBoxRef.current || !subTextRef.current) {
      setMarquee({ overflowPx: 0, durationSec: 0 });
      setMarqueeActive(false);
      return;
    }

    const measure = () => {
      if (!subBoxRef.current || !subTextRef.current) return;
      const overflowPx = Math.ceil(
        subTextRef.current.scrollWidth - subBoxRef.current.clientWidth,
      );
      setMarquee({
        overflowPx: Math.max(0, overflowPx),
        durationSec: overflowPx > 0 ? Math.max(2.6, overflowPx / 42) : 0,
      });
    };

    measure();
    const frame = window.requestAnimationFrame(measure);
    return () => window.cancelAnimationFrame(frame);
  }, [hov, sub]);

  React.useEffect(() => {
    if (marqueeTimerRef.current != null) {
      window.clearTimeout(marqueeTimerRef.current);
      marqueeTimerRef.current = null;
    }

    if (!hov || marquee.overflowPx <= 0) {
      setMarqueeActive(false);
      return;
    }

    marqueeTimerRef.current = window.setTimeout(() => {
      setMarqueeActive(true);
    }, 1050);

    return () => {
      if (marqueeTimerRef.current != null) {
        window.clearTimeout(marqueeTimerRef.current);
        marqueeTimerRef.current = null;
      }
    };
  }, [hov, marquee.overflowPx]);

  return (
    <div
      onMouseEnter={() => setHov(true)}
      onMouseLeave={() => setHov(false)}
      className="settings-row relative flex items-center justify-between px-4 py-[13px] overflow-visible"
      style={{
        borderBottom: last ? "none" : `1px solid rgba(255,255,255,.04)`,
        minHeight: 50,
      }}
    >
      <div className="flex-1 min-w-0 pr-3">
        <div className="flex items-center gap-2 min-w-0">
          <p className="shrink-0 text-[13px] text-t1 font-normal leading-[1.3]">
            {label}
          </p>
          {sub && (
            <span
              ref={subBoxRef}
              className="settings-sub min-w-0 overflow-hidden whitespace-nowrap text-[11px] leading-[1.3] pointer-events-none"
              style={{
                opacity: hov ? 1 : 0,
                transform: hov
                  ? "translate3d(0, 1px, 0)"
                  : "translate3d(-3px, 1px, 0)",
                color: "rgba(215,212,204,.78)",
                ["--settings-sub-overflow" as string]: `${marquee.overflowPx}px`,
                ["--settings-sub-duration" as string]: `${marquee.durationSec}s`,
                transition:
                  "opacity .16s cubic-bezier(.2,0,0,1), transform .16s cubic-bezier(.2,0,0,1)",
              }}
            >
              <span
                ref={subTextRef}
                className={
                  marqueeActive
                    ? "settings-sub-track is-marquee"
                    : "settings-sub-track"
                }
              >
                {sub}
              </span>
            </span>
          )}
        </div>
      </div>
      <div className="shrink-0 overflow-visible">{children}</div>
    </div>
  );
}

// ── Toggle ───────────────────────────────────────────────────────────────────

/** トグルスイッチ */
export function Toggle({ on, onToggle }: { on: boolean; onToggle: () => void }) {
  return (
    <button
      onClick={onToggle}
      className="toggle-root flex items-center leading-none shrink-0 cursor-pointer p-0 bg-transparent border-0"
    >
      <span
        className={
          on
            ? "toggle-track on inline-flex w-9 h-5 rounded-[10px] relative shrink-0 transition-[background,box-shadow] duration-200"
            : "toggle-track inline-flex w-9 h-5 rounded-[10px] relative shrink-0 transition-[background,box-shadow] duration-200"
        }
        style={{
          background: on ? C.green : "rgba(255,255,255,.10)",
          boxShadow: "inset 0 0 0 1px rgba(255,255,255,.035)",
          transitionTimingFunction: "cubic-bezier(.2,0,0,1)",
        }}
      >
        <span
          className="absolute top-[3px] w-[14px] h-[14px] rounded-[7px] bg-white transition-[left,box-shadow] duration-200"
          style={{
            left: on ? 19 : 3,
            boxShadow: on
              ? "0 2px 6px rgba(0,0,0,.32), 0 0 10px rgba(255,255,255,.18)"
              : "0 1px 3px rgba(0,0,0,.35)",
            transitionTimingFunction: "cubic-bezier(.2,0,0,1)",
          }}
        />
      </span>
    </button>
  );
}

// ── SBtn ─────────────────────────────────────────────────────────────────────

/** 小さなアクションボタン */
export function SBtn({
  children,
  onClick,
  disabled,
  tone,
  icon,
}: {
  children: React.ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  tone?: "amber" | "blue";
  icon?: React.ReactNode;
}) {
  const bg =
    tone === "amber"
      ? "rgba(232,162,58,.1)"
      : tone === "blue"
        ? "rgba(90,158,255,.1)"
        : "rgba(255,255,255,.06)";
  const col = tone === "amber" ? C.warning : tone === "blue" ? "#5a9eff" : C.t2;
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className="btn-phys flex items-center gap-1 px-[10px] py-[5px] rounded-[6px] cursor-pointer text-[10px] font-normal transition-opacity duration-[120ms]"
      style={{
        background: bg,
        border: "none",
        color: col,
        opacity: disabled ? 0.5 : 1,
      }}
    >
      {icon}
      {children}
    </button>
  );
}

// ── CodeLine ─────────────────────────────────────────────────────────────────

/** クリックでコピーできるコードスニペット */
export function CodeLine({ code }: { code: string }) {
  const { t } = useTranslation();
  return (
    <div
      className="flex items-center justify-between rounded-[7px] px-[11px] py-[7px] mb-[10px] cursor-pointer bg-[rgba(0,0,0,.4)]"
      onClick={() => navigator.clipboard.writeText(code)}
    >
      <code
        className="text-green text-[10px]"
        style={{ fontFamily: "'JetBrains Mono','SF Mono','Fira Code',monospace" }}
      >
        {code}
      </code>
      <span className="text-t3 text-[9px] ml-2 shrink-0">
        {t("ui.copy_snippet")}
      </span>
    </div>
  );
}

// ── DebugSection ─────────────────────────────────────────────────────────────

/** デバッグパネル用セクション */
export function DebugSection({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div
      className="rounded-[6px] px-[14px] py-3 bg-[rgba(0,0,0,.22)]"
    >
      <div
        className="text-[10px] font-semibold tracking-[0.10em] text-green uppercase mb-[10px] opacity-70"
      >
        {title}
      </div>
      <div className="flex flex-col gap-[6px]">{children}</div>
    </div>
  );
}

// ── DebugRow ─────────────────────────────────────────────────────────────────

/** デバッグパネル用行 */
export function DebugRow({
  k,
  v,
  mono,
  highlight,
  warn,
  copyable,
  openable,
}: {
  k: string;
  v: string;
  mono?: boolean;
  highlight?: boolean;
  warn?: boolean;
  copyable?: boolean;
  openable?: boolean;
}) {
  const [copied, setCopied] = React.useState(false);
  const color = highlight ? C.green : warn ? C.warning : C.t1;
  return (
    <div
      className="debug-row flex items-start gap-2 min-h-5"
    >
      <span
        className="text-[10px] text-t3 w-[140px] shrink-0 pt-[1px] tracking-[0.01em] leading-[1.5]"
      >
        {k}
      </span>
      <span
        className="text-[10px] flex-1 break-all leading-[1.5]"
        style={{
          color,
          fontFamily: mono ? "'JetBrains Mono','SF Mono','Fira Code',monospace" : "inherit",
        }}
      >
        {v || "—"}
      </span>
      {copyable && v && (
        <button
          onClick={() => {
            navigator.clipboard.writeText(v);
            setCopied(true);
            setTimeout(() => setCopied(false), 1500);
          }}
          className={`debug-row-action flex items-center rounded-[3px] cursor-pointer text-[9px] px-[5px] py-[3px] shrink-0 transition-[opacity,color] duration-[120ms] ${
            copied ? "copied" : ""
          }`}
          style={{
            background: "none",
            border: `1px solid ${C.b1}`,
            color: copied ? C.green : C.t3,
          }}
        >
          <span className="inline-flex w-[9px] h-[9px] items-center justify-center text-[9px] leading-none">
            {copied ? "✓" : <Copy size={9} />}
          </span>
        </button>
      )}
      {openable && v && (
        <button
          onClick={() => invoke("open_folder", { path: v }).catch(console.error)}
          className="debug-row-action flex items-center rounded-[3px] cursor-pointer text-[9px] px-[5px] py-[3px] shrink-0 transition-[opacity,color] duration-[120ms]"
          style={{
            background: "none",
            border: `1px solid ${C.b1}`,
            color: C.t3,
          }}
          onMouseEnter={(e) => ((e.currentTarget as HTMLButtonElement).style.color = C.t2)}
          onMouseLeave={(e) => ((e.currentTarget as HTMLButtonElement).style.color = C.t3)}
        >
          <FolderOpen size={9} />
        </button>
      )}
    </div>
  );
}
