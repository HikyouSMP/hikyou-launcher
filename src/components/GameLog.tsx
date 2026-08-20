import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Check, Copy, Search, Trash2, X } from "lucide-react";
import {
  type MutableRefObject,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import i18n from "../i18n";
import { C } from "../theme";
import type { CrashAnalysis, LauncherSettings, ParsedCrash, Profile } from "../types";
import { LogDropdown } from "./LogDropdown";

type WindowMode = "log" | "crash";
type Level = "INFO" | "WARN" | "ERROR" | "FATAL" | "DEBUG" | "OTHER";
type Filter = "ALL" | "WARN" | "ERROR";

interface LogLine {
  id: number;
  time: string;
  thread: string;
  level: Level;
  body: string;
  raw: string;
}

interface LogSource {
  id: string;
  label: string;
  kind: string;
  source_path: string;
  modified_ms: number;
}

const RE = /^\[(\d{2}:\d{2}:\d{2})\] \[([^\]]+?)\/([A-Z]+)\]: (.*)$/;

function initialProfileId() {
  return new URLSearchParams(window.location.search).get("profileId");
}

function parseLine(raw: string, id: number): LogLine {
  const m = raw.match(RE);
  if (m) {
    const lvl = m[3] as Level;
    return {
      id,
      time: m[1],
      thread: m[2],
      level: ["INFO", "WARN", "ERROR", "FATAL", "DEBUG"].includes(lvl)
        ? lvl
        : "OTHER",
      body: m[4],
      raw,
    };
  }
  if (
    /^A detailed walkthrough of the error/i.test(raw) ||
    /^\s*Suppressed Exceptions:\s*~~NONE~~/i.test(raw) ||
    /^\s*mixinextras:\s*MixinExtras\b/i.test(raw)
  ) {
    return { id, time: "", thread: "", level: "OTHER", body: raw, raw };
  }
  const upper = raw.toUpperCase();
  const level = upper.includes("FATAL")
    ? "FATAL"
    : /\bERROR\b/.test(upper) ||
        /\b[A-Z0-9_.$]+(?:EXCEPTION|ERROR)(?::|\b)/.test(upper)
      ? "ERROR"
      : upper.includes("WARN")
        ? "WARN"
        : "OTHER";
  return { id, time: "", thread: "", level, body: raw, raw };
}

const LEVEL_STYLE: Record<Level, { color: string; bg: string; label: string }> =
  {
    INFO: { color: C.t3, bg: "transparent", label: "INFO" },
    DEBUG: { color: C.t3, bg: "transparent", label: "DEBUG" },
    OTHER: { color: C.t3, bg: "transparent", label: "" },
    WARN: { color: C.warning, bg: "rgba(184,144,48,.08)", label: "WARN" },
    ERROR: { color: C.danger, bg: "rgba(122,58,50,.12)", label: "ERROR" },
    FATAL: { color: C.danger, bg: "rgba(122,58,50,.16)", label: "FATAL" },
  };

export function GameLog({ mode }: { mode: WindowMode }) {
  const { t } = useTranslation();
  const [profiles, setProfiles] = useState<Profile[]>([]);
  const [selectedProfileId, setSelectedProfileId] = useState<string | null>(
    initialProfileId(),
  );
  const [logsByProfileSource, setLogsByProfileSource] = useState<
    Record<string, Record<string, string[]>>
  >({});
  const [logRefreshToken, setLogRefreshToken] = useState(0);
  const [sourceRefreshToken, setSourceRefreshToken] = useState(0);
  const [sourcesByProfile, setSourcesByProfile] = useState<
    Record<string, LogSource[]>
  >({});
  const [selectedSourcePath, setSelectedSourcePath] = useState<string | null>(
    null,
  );
  const [filter, setFilter] = useState<Filter>("ALL");
  const [query, setQuery] = useState("");
  const [regexMode, setRegexMode] = useState(false);
  const [, setAutoScroll] = useState(true);
  const [copied, setCopied] = useState(false);
  const [sideWidth, setSideWidth] = useState(260);
  const [logDiagnosis, setLogDiagnosis] = useState<ParsedCrash | null>(null);
  const [highlightedLine, setHighlightedLine] = useState<number | null>(null);
  const [suspectTip, setSuspectTip] = useState<{
    text: string;
    x: number;
    y: number;
  } | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  const rowRefs = useRef<Map<number, HTMLDivElement>>(new Map());
  const pendingJumpRef = useRef<number | null>(null);
  const refreshTimerRef = useRef<number | null>(null);
  const activeProfileIdRef = useRef<string | null>(null);
  const selectedProfileIdRef = useRef<string | null>(selectedProfileId);
  const selectedSourcePathRef = useRef<string | null>(selectedSourcePath);
  const latestSourcePathRef = useRef<string | null>(null);
  const selectedSourceIsLatestRef = useRef(false);
  const autoScrollRef = useRef(true);

  const activeProfileId = selectedProfileId ?? profiles[0]?.id ?? null;
  const activeProfile = profiles.find((p) => p.id === activeProfileId);

  const scheduleLogRefresh = useCallback((refreshSources = false) => {
    if (refreshTimerRef.current != null) {
      window.clearTimeout(refreshTimerRef.current);
    }
    refreshTimerRef.current = window.setTimeout(() => {
      setLogRefreshToken((value) => value + 1);
      if (refreshSources) setSourceRefreshToken((value) => value + 1);
      refreshTimerRef.current = null;
    }, 450);
  }, []);

  useEffect(() => {
    activeProfileIdRef.current = activeProfileId;
  }, [activeProfileId]);
  useEffect(() => {
    selectedProfileIdRef.current = selectedProfileId;
  }, [selectedProfileId]);
  useEffect(() => {
    selectedSourcePathRef.current = selectedSourcePath;
  }, [selectedSourcePath]);

  useEffect(() => {
    invoke<LauncherSettings>("get_settings")
      .then((settings) => {
        const locale = settings.ui?.locale;
        if (locale && locale !== i18n.language) {
          i18n.changeLanguage(locale).catch(console.error);
        }
      })
      .catch(console.error);
  }, []);

  useEffect(() => {
    invoke<Profile[]>("list_profiles")
      .then((items) => {
        setProfiles(items);
        if (!selectedProfileId && items[0]) setSelectedProfileId(items[0].id);
      })
      .catch(console.error);
  }, [selectedProfileId]);

  useEffect(() => {
    if (!activeProfileId) return;
    invoke<LogSource[]>("list_profile_log_sources", {
      profileId: activeProfileId,
    })
      .then((sources) => {
        setSourcesByProfile((prev) => ({ ...prev, [activeProfileId]: sources }));
        const currentSource = selectedSourcePathRef.current;
        const currentStillExists = sources.some(
          (source) => source.source_path === currentSource,
        );
        if (!currentSource || !currentStillExists) {
          const preferred =
            sources.find((source) => source.label === "latest.log") ??
            sources[0];
          if (preferred) setSelectedSourcePath(preferred.source_path);
        }
      })
      .catch(console.error);
  }, [activeProfileId, sourceRefreshToken]);

  useEffect(() => {
    if (!activeProfileId || !selectedSourcePath) return;
    invoke<string[]>("read_profile_log_source", {
      profileId: activeProfileId,
      sourcePath: selectedSourcePath,
    })
      .then((lines) => {
        setLogsByProfileSource((prev) => ({
          ...prev,
          [activeProfileId]: {
            ...(prev[activeProfileId] ?? {}),
            [selectedSourcePath]: lines,
          },
        }));
      })
      .catch(console.error);
  }, [activeProfileId, selectedSourcePath, logRefreshToken]);

  useEffect(() => {
    const unlistenLog = listen<{ profile_id: string; line: string } | string>(
      "game://log",
      ({ payload }) => {
        const currentProfileId = activeProfileIdRef.current;
        const pid =
          typeof payload === "string"
            ? (currentProfileId ?? "__unknown")
            : (payload.profile_id ?? "__unknown");
        const line = typeof payload === "string" ? payload : payload.line;
        if (!selectedProfileIdRef.current) setSelectedProfileId(pid);
        if (pid === currentProfileId) {
          const latestPath = latestSourcePathRef.current;
          if (latestPath && selectedSourceIsLatestRef.current) {
            setLogsByProfileSource((prev) => {
              const bySource = prev[pid] ?? {};
              const current = bySource[latestPath] ?? [];
              const next = [...current, line];
              return {
                ...prev,
                [pid]: {
                  ...bySource,
                  [latestPath]:
                    next.length > 5000 ? next.slice(next.length - 5000) : next,
                },
              };
            });
          }
          scheduleLogRefresh(true);
        }
      },
    );
    const unlistenExit = listen<{ profile_id?: string } | string>(
      "game://exit",
      ({ payload }) => {
        const currentProfileId = activeProfileIdRef.current;
        const pid =
          typeof payload === "string"
            ? payload
            : (payload.profile_id ?? currentProfileId);
        if (pid === currentProfileId) scheduleLogRefresh(true);
      },
    );
    const unlistenSelect = listen<string>("log://select-profile", ({ payload }) => {
      setSelectedProfileId(payload);
      setSourceRefreshToken((value) => value + 1);
    });
    return () => {
      if (refreshTimerRef.current != null) {
        window.clearTimeout(refreshTimerRef.current);
      }
      unlistenLog.then((f) => f());
      unlistenExit.then((f) => f());
      unlistenSelect.then((f) => f());
    };
  }, [scheduleLogRefresh]);

  const rawLines = activeProfileId
    ? selectedSourcePath == null
      ? []
      : (logsByProfileSource[activeProfileId]?.[selectedSourcePath] ?? [])
    : [];
  const logSources = activeProfileId ? (sourcesByProfile[activeProfileId] ?? []) : [];
  useEffect(() => {
    const latest = logSources.find((source) => source.label === "latest.log");
    latestSourcePathRef.current = latest?.source_path ?? null;
    selectedSourceIsLatestRef.current =
      selectedSourcePath != null && selectedSourcePath === latest?.source_path;
  }, [logSources, selectedSourcePath]);
  const sourceOptions = useMemo(() => {
    return logSources.map((source) => ({
        value: source.source_path,
        label: source.label,
      }));
  }, [logSources]);

  useEffect(() => {
    if (!activeProfileId) return;
    if (selectedSourcePath == null) {
      const preferred =
        logSources.find((source) => source.label === "latest.log") ??
        logSources[0];
      if (preferred) setSelectedSourcePath(preferred.source_path);
    }
  }, [activeProfileId, logSources, selectedSourcePath]);
  const lines = useMemo(
    () => rawLines.slice(-5000).map((line, i) => parseLine(line, i)),
    [rawLines],
  );

  useEffect(() => {
    if (rawLines.length === 0) {
      setLogDiagnosis(null);
      return;
    }
    const timer = window.setTimeout(() => {
      invoke<ParsedCrash>("parse_crash_log", {
        logLines: rawLines.slice(-5000),
        lang: i18n.language?.startsWith("ja") ? "ja" : "en",
      })
        .then(setLogDiagnosis)
        .catch(() => setLogDiagnosis(null));
    }, 350);
    return () => window.clearTimeout(timer);
  }, [rawLines]);

  useLayoutEffect(() => {
    if (!autoScrollRef.current) return;
    const el = bodyRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
    window.requestAnimationFrame(() => {
      el.scrollTop = el.scrollHeight;
    });
  }, [lines.length]);

  const matcher = useMemo(() => {
    if (!query.trim()) return null;
    if (!regexMode) return (line: LogLine) => line.raw.toLowerCase().includes(query.toLowerCase());
    try {
      const re = new RegExp(query, "i");
      return (line: LogLine) => re.test(line.raw);
    } catch {
      return () => true;
    }
  }, [query, regexMode]);

  const visible = lines.filter((line) => {
    if (filter === "WARN" && line.level !== "WARN") return false;
    if (filter === "ERROR" && !["ERROR", "FATAL"].includes(line.level))
      return false;
    if (matcher && !matcher(line)) return false;
    return true;
  });

  useEffect(() => {
    const target = pendingJumpRef.current;
    if (target == null) return;
    const el = rowRefs.current.get(target);
    if (!el) return;
    pendingJumpRef.current = null;
    el.scrollIntoView({ block: "center" });
    setHighlightedLine(target);
    window.setTimeout(() => setHighlightedLine((cur) => (cur === target ? null : cur)), 1700);
  }, [visible.length, visible]);
  const warnCount = lines.filter((l) => l.level === "WARN").length;
  const errorCount = lines.filter((l) => ["ERROR", "FATAL"].includes(l.level)).length;
  const threadStats = useMemo(() => {
    const counts = new Map<string, number>();
    for (const line of lines) {
      if (!line.thread) continue;
      counts.set(line.thread, (counts.get(line.thread) ?? 0) + 1);
    }
    return Array.from(counts.entries())
      .sort((a, b) => b[1] - a[1])
      .slice(0, 6);
  }, [lines]);
  const suspicious = useMemo(() => {
    const seen = new Set<number>();
    const fromEvidence =
      logDiagnosis?.diagnosis.evidence
        .map((evidence) => {
          const needle = evidence.trim();
          if (!needle) return null;
          const line = lines.find((candidate) => {
            const raw = candidate.raw.trim();
            if (!raw) return false;
            return candidate.raw.includes(needle) || needle.includes(raw);
          });
          if (!line || seen.has(line.id)) return null;
          seen.add(line.id);
          return { line, reason: evidence, source: "evidence" as const };
        })
        .filter(Boolean) ?? [];
    if (fromEvidence.length > 0) {
      return fromEvidence as Array<{
        line: LogLine;
        reason: string;
        source: "evidence" | "heuristic";
      }>;
    }
    return lines
      .filter((line) =>
        /exception|caused by|failed|missing|requires|incompatible|unsupported|mod loading|mixin apply/i.test(
          line.raw,
        ),
      )
      .filter(
        (line) =>
          !/^A detailed walkthrough of the error/i.test(line.raw) &&
          !/^\s*Suppressed Exceptions:\s*~~NONE~~/i.test(line.raw) &&
          !/^\s*mixinextras:\s*MixinExtras\b/i.test(line.raw),
      )
      .slice(-16)
      .map((line) => ({ line, reason: "heuristic", source: "heuristic" as const }));
  }, [lines, logDiagnosis]);

  const copyText = (text: string) => {
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  };

  const jumpToLine = (lineId: number) => {
    pendingJumpRef.current = lineId;
    setFilter("ALL");
    setQuery("");
    setRegexMode(false);
    window.setTimeout(() => {
      const el = rowRefs.current.get(lineId);
      if (!el) return;
      pendingJumpRef.current = null;
      el.scrollIntoView({ block: "center" });
      setHighlightedLine(null);
      requestAnimationFrame(() => setHighlightedLine(lineId));
      window.setTimeout(
        () => setHighlightedLine((cur) => (cur === lineId ? null : cur)),
        1700,
      );
    }, 30);
  };

  const startResize = (event: React.MouseEvent) => {
    event.preventDefault();
    const startX = event.clientX;
    const startWidth = sideWidth;
    const onMove = (moveEvent: MouseEvent) => {
      const next = Math.min(420, Math.max(220, startWidth + moveEvent.clientX - startX));
      setSideWidth(next);
    };
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  const close = () => {
    const win = getCurrentWindow();
    win.close().catch(() => win.hide().catch(console.error));
  };

  if (mode === "crash") {
    return (
      <div className="tool-window crash-window">
        <WindowHeader title="Minecraft Crash" onClose={close} />
        <CrashSummary
          crash={undefined}
          activeProfile={activeProfile}
          onCopy={() => copyText(rawLines.join("\n"))}
          copied={copied}
        />
      </div>
    );
  }

  return (
    <div className="tool-window log-inspector-window">
      <div
        className="log-inspector-grid"
        style={{ gridTemplateColumns: `${sideWidth}px 6px minmax(0, 1fr)` }}
      >
        <aside className="log-sidebar">
          <label className="log-side-label">{t("log.profile")}</label>
          <LogDropdown
            value={activeProfileId ?? ""}
            options={profiles.map((profile) => ({
              value: profile.id,
              label: profile.name,
            }))}
            onChange={setSelectedProfileId}
          />
          <label className="log-side-label mt">{t("log.sources")}</label>
          <LogDropdown
            value={selectedSourcePath ?? ""}
            options={sourceOptions}
            onChange={setSelectedSourcePath}
          />
          <div className="log-side-card">
            <div className="log-side-title">{t("log.signals")}</div>
            <Metric label={t("log.lines")} value={lines.length} />
            <Metric label={t("log.warnings")} value={warnCount} tone="warn" />
            <Metric label={t("log.errors")} value={errorCount} tone="error" />
          </div>
          <div className="log-side-card">
            <div className="log-side-title">{t("log.threads")}</div>
            {threadStats.length === 0 ? (
              <span className="log-muted">{t("log.no_thread_data")}</span>
            ) : (
              threadStats.map(([thread, count]) => (
                <Metric key={thread} label={thread} value={count} />
              ))
            )}
          </div>
          <div className="log-side-card">
            <div className="log-side-title">{t("log.suspect_lines")}</div>
            {suspicious.length === 0 ? (
              <span className="log-muted">{t("log.no_suspects")}</span>
            ) : (
              suspicious.slice(-6).map(({ line, reason, source }) => (
                <button
                  key={line.id}
                  className="suspect-link"
                  onClick={() => jumpToLine(line.id)}
                  onMouseEnter={(event) =>
                    setSuspectTip({
                      text: reason === "heuristic" ? line.raw : reason,
                      x: event.clientX,
                      y: event.clientY,
                    })
                  }
                  onMouseMove={(event) =>
                    setSuspectTip((tip) =>
                      tip
                        ? { ...tip, x: event.clientX, y: event.clientY }
                        : tip,
                    )
                  }
                  onMouseLeave={() => setSuspectTip(null)}
                >
                  <span>{line.id + 1}</span>
                  <b>
                    <small>{source}</small>
                    {line.raw}
                  </b>
                </button>
              ))
            )}
          </div>
          {suspectTip && (
            <div
              className="log-tooltip"
              style={{
                left: Math.min(window.innerWidth - 440, suspectTip.x + 14),
                top: Math.min(window.innerHeight - 160, suspectTip.y + 14),
              }}
            >
              {suspectTip.text}
            </div>
          )}
        </aside>
        <div
          className="log-resizer"
          onMouseDown={startResize}
          title={t("log.resize_sidebar")}
        />
        <main className="log-main">
          <div className="log-window-head detached">
            <div className="log-search">
              <Search size={13} />
              <input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={t("log.search_placeholder")}
              />
            </div>
            <button
              onClick={() => setRegexMode((v) => !v)}
              className={regexMode ? "log-filter-btn active" : "log-filter-btn"}
              title={t("log.regex_hint")}
            >
              {t("log.regex")}
            </button>
            {(["ALL", "WARN", "ERROR"] as Filter[]).map((f) => (
              <button
                key={f}
                onClick={() => setFilter(f)}
                className={filter === f ? "log-filter-btn active" : "log-filter-btn"}
              >
                {f === "ALL" ? t("log.filter_all") : f}
              </button>
            ))}
            <button
              className={copied ? "log-icon-btn copied" : "log-icon-btn"}
              onClick={() => copyText(visible.map((l) => l.raw).join("\n"))}
              title={copied ? t("common.copied") : t("common.copy_all")}
            >
              {copied ? <Check size={14} /> : <Copy size={14} />}
            </button>
            <button
              className="log-icon-btn"
              onClick={() => {
                if (!activeProfileId) return;
                if (selectedSourcePath != null) {
                  setLogsByProfileSource((prev) => ({
                    ...prev,
                    [activeProfileId]: {
                      ...(prev[activeProfileId] ?? {}),
                      [selectedSourcePath]: [],
                    },
                  }));
                }
              }}
            >
              <Trash2 size={13} />
            </button>
          </div>
          <div
            ref={bodyRef}
            onScroll={() => {
              const el = bodyRef.current;
              if (!el) return;
              const next = el.scrollHeight - el.scrollTop - el.clientHeight < 64;
              autoScrollRef.current = next;
              setAutoScroll(next);
            }}
            className="log-window-body detached sb"
          >
            {visible.length === 0 ? (
              <div className="py-8 px-4 text-center text-t3 text-xs">
                {lines.length === 0 ? t("log.empty") : t("log.empty_filtered")}
              </div>
            ) : (
              visible.map((line) => (
                <LogRow
                  key={line.id}
                  line={line}
                  rowRefs={rowRefs}
                  highlighted={highlightedLine === line.id}
                />
              ))
            )}
            <div ref={bottomRef} />
          </div>
        </main>
      </div>
    </div>
  );
}

function WindowHeader({ title, onClose }: { title: string; onClose: () => void }) {
  return (
    <div className="tool-window-titlebar">
      <span>{title}</span>
      <button onClick={onClose} className="log-icon-btn">
        <X size={14} />
      </button>
    </div>
  );
}

function CrashSummary({
  crash,
  activeProfile,
  compact,
  copied,
  onCopy,
}: {
  crash?: CrashAnalysis;
  activeProfile?: Profile;
  compact?: boolean;
  copied: boolean;
  onCopy: () => void;
}) {
  const { t } = useTranslation();
  if (!crash) {
    return (
      <div className="crash-summary">
        <p className="text-[12px] text-t2">No crash analysis is available yet.</p>
      </div>
    );
  }
  return (
    <div className={compact ? "crash-summary compact" : "crash-summary"}>
      <div className="flex items-center gap-2">
        <span className="crash-chip">{crash.parsed.diagnosis.category}</span>
        <span className="text-[10px] text-t3 font-mono">
          {Math.round(crash.parsed.diagnosis.confidence * 100)}%
        </span>
        {activeProfile && (
          <span className="text-[10px] text-t3 ml-auto">
            {activeProfile.mcVersion} / {activeProfile.loader}
          </span>
        )}
      </div>
      <p className="text-[12px] leading-[1.65] text-t1 mt-2">
        {crash.parsed.diagnosis.summary}
      </p>
      {crash.parsed.diagnosis.actions.length > 0 && (
        <div className="flex flex-wrap gap-1.5 mt-2">
          {crash.parsed.diagnosis.actions.map((action) => (
            <span key={action.kind} className="crash-action" title={action.detail}>
              {action.label}
            </span>
          ))}
        </div>
      )}
      {!compact && (
        <div className="flex gap-1.5 mt-3">
          <button className="crash-action button" onClick={onCopy}>
            <Copy size={11} /> {copied ? t("common.copied") : t("common.copy_all")}
          </button>
        </div>
      )}
      {crash.parsed.diagnosis.evidence.length > 0 && (
        <details className="mt-2">
          <summary className="text-[10px] text-t3 cursor-pointer select-none">
            {t("log.evidence")}
          </summary>
          <div className="mt-1.5 flex flex-col gap-0.5">
            {crash.parsed.diagnosis.evidence.map((line, i) => (
              <code key={i} className="crash-evidence">
                {line}
              </code>
            ))}
          </div>
        </details>
      )}
    </div>
  );
}

function Metric({
  label,
  value,
  tone,
}: {
  label: string;
  value: number;
  tone?: "warn" | "error";
}) {
  return (
    <div className="log-metric">
      <span>{label}</span>
      <b className={tone ?? ""}>{value}</b>
    </div>
  );
}

function LogRow({
  line,
  rowRefs,
  highlighted,
}: {
  line: LogLine;
  rowRefs: MutableRefObject<Map<number, HTMLDivElement>>;
  highlighted: boolean;
}) {
  const s = LEVEL_STYLE[line.level];
  return (
    <div
      ref={(el) => {
        if (el) rowRefs.current.set(line.id, el);
        else rowRefs.current.delete(line.id);
      }}
      className={highlighted ? "log-row highlighted" : "log-row"}
      style={{ background: s.bg }}
    >
      <span className="log-line-no">{line.id + 1}</span>
      {line.time && <span className="log-time">{line.time}</span>}
      {line.level !== "INFO" && line.level !== "OTHER" && line.level !== "DEBUG" && (
        <span className="log-level" style={{ color: s.color }}>
          {s.label}
        </span>
      )}
      <span className="break-all" style={{ color: s.color === C.t3 ? C.t2 : s.color }}>
        {line.body || line.raw}
      </span>
    </div>
  );
}
