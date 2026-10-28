<script lang="ts">
  import { onDestroy, onMount, tick, untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { Check, Copy, Search, Trash2, X } from "@lucide/svelte";
  import { createTranslation } from "../lib/i18n.svelte";
  import type { LauncherSettings, ParsedCrash, Profile } from "../types";
  import LogDropdown from "./LogDropdown.svelte";
  import GameLogRow from "./GameLogRow.svelte";
  import GameLogCrashSummary from "./GameLogCrashSummary.svelte";
  import type { Filter, Level, LogLine, LogSource, WindowMode } from "./GameLog.types";

  let { mode }: { mode: WindowMode } = $props();
  const translation = createTranslation();
  const { t, i18n } = translation;
  let profiles = $state<Profile[]>([]);
  let selectedProfileId = $state<string | null>(new URLSearchParams(window.location.search).get("profileId"));
  let logsByProfileSource = $state<Record<string, Record<string, string[]>>>({});
  let logRefreshToken = $state(0);
  let sourceRefreshToken = $state(0);
  let sourcesByProfile = $state<Record<string, LogSource[]>>({});
  let selectedSourcePath = $state<string | null>(null);
  let filter = $state<Filter>("ALL");
  let query = $state("");
  let regexMode = $state(false);
  let copied = $state(false);
  let sideWidth = $state(260);
  let logDiagnosis = $state<ParsedCrash | null>(null);
  let highlightedLine = $state<number | null>(null);
  let suspectTip = $state<{ text: string; x: number; y: number } | null>(null);
  let body = $state<HTMLDivElement>();
  const rows = new Map<number, HTMLDivElement>();
  let autoScroll = true;
  let refreshTimer: ReturnType<typeof setTimeout> | undefined;
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;
  let highlightTimer: ReturnType<typeof setTimeout> | undefined;
  let scrollFrame: number | undefined;
  let jumpFrame: number | undefined;
  let jumpSequence = 0;
  let destroyed = false;
  let resize: { x: number; width: number } | null = null;

  function parseLine(raw: string, id: number): LogLine {
    const match = raw.match(/^\[(\d{2}:\d{2}:\d{2})\] \[([^\]]+?)\/([A-Z]+)\]: (.*)$/);
    if (match) {
      const level = match[3] as Level;
      return { id, time: match[1], thread: match[2], level: ["INFO", "WARN", "ERROR", "FATAL", "DEBUG"].includes(level) ? level : "OTHER", body: match[4], raw };
    }
    if (/^A detailed walkthrough of the error/i.test(raw) || /^\s*Suppressed Exceptions:\s*~~NONE~~/i.test(raw) || /^\s*mixinextras:\s*MixinExtras\b/i.test(raw)) {
      return { id, time: "", thread: "", level: "OTHER", body: raw, raw };
    }
    const upper = raw.toUpperCase();
    const level = upper.includes("FATAL") ? "FATAL" : /\bERROR\b/.test(upper) || /\b[A-Z0-9_.$]+(?:EXCEPTION|ERROR)(?::|\b)/.test(upper) ? "ERROR" : upper.includes("WARN") ? "WARN" : "OTHER";
    return { id, time: "", thread: "", level, body: raw, raw };
  }

  const activeProfileId = $derived(selectedProfileId ?? profiles[0]?.id ?? null);
  const activeProfile = $derived(profiles.find((profile) => profile.id === activeProfileId));
  const logSources = $derived(activeProfileId ? sourcesByProfile[activeProfileId] ?? [] : []);
  const latestSourcePath = $derived(logSources.find((source) => source.label === "latest.log")?.source_path ?? null);
  const sourceOptions = $derived(logSources.map((source) => ({ value: source.source_path, label: source.label })));
  const rawLines = $derived(activeProfileId && selectedSourcePath ? logsByProfileSource[activeProfileId]?.[selectedSourcePath] ?? [] : []);
  const lines = $derived(rawLines.slice(-5000).map(parseLine));
  const warnCount = $derived(lines.filter((line) => line.level === "WARN").length);
  const errorCount = $derived(lines.filter((line) => ["ERROR", "FATAL"].includes(line.level)).length);
  const matcher = $derived.by(() => {
    if (!query.trim()) return null;
    if (!regexMode) return (line: LogLine) => line.raw.toLowerCase().includes(query.toLowerCase());
    try { const expression = new RegExp(query, "i"); return (line: LogLine) => expression.test(line.raw); }
    catch { return () => true; }
  });
  const visible = $derived(lines.filter((line) => {
    if (filter === "WARN" && line.level !== "WARN") return false;
    if (filter === "ERROR" && !["ERROR", "FATAL"].includes(line.level)) return false;
    return !matcher || matcher(line);
  }));
  const threadStats = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const line of lines) if (line.thread) counts.set(line.thread, (counts.get(line.thread) ?? 0) + 1);
    return Array.from(counts.entries()).sort((a, b) => b[1] - a[1]).slice(0, 6);
  });
  const suspicious = $derived.by(() => {
    const seen = new Set<number>();
    const evidenceLines: Array<{ line: LogLine; reason: string; source: "evidence" | "heuristic" }> = [];
    for (const evidence of logDiagnosis?.diagnosis.evidence ?? []) {
      const needle = evidence.trim();
      if (!needle) continue;
      const line = lines.find((candidate) => candidate.raw.trim() && (candidate.raw.includes(needle) || needle.includes(candidate.raw.trim())));
      if (!line || seen.has(line.id)) continue;
      seen.add(line.id);
      evidenceLines.push({ line, reason: evidence, source: "evidence" });
    }
    if (evidenceLines.length) return evidenceLines;
    return lines.filter((line) => /exception|caused by|failed|missing|requires|incompatible|unsupported|mod loading|mixin apply/i.test(line.raw))
      .filter((line) => !/^A detailed walkthrough of the error/i.test(line.raw) && !/^\s*Suppressed Exceptions:\s*~~NONE~~/i.test(line.raw) && !/^\s*mixinextras:\s*MixinExtras\b/i.test(line.raw))
      .slice(-16).map((line) => ({ line, reason: "heuristic", source: "heuristic" as const }));
  });

  function scheduleLogRefresh(refreshSources = false) {
    clearTimeout(refreshTimer);
    refreshTimer = setTimeout(() => {
      logRefreshToken += 1;
      if (refreshSources) sourceRefreshToken += 1;
      refreshTimer = undefined;
    }, 450);
  }

  onMount(() => {
    invoke<LauncherSettings>("get_settings").then((settings) => {
      const locale = settings.ui?.locale;
      if (!destroyed && locale && locale !== i18n.language) i18n.changeLanguage(locale).catch(console.error);
    }).catch(console.error);
    const listeners = [
      listen<{ profile_id: string; line: string } | string>("game://log", ({ payload }) => {
        const pid = typeof payload === "string" ? activeProfileId ?? "__unknown" : payload.profile_id ?? "__unknown";
        const line = typeof payload === "string" ? payload : payload.line;
        if (!selectedProfileId) selectedProfileId = pid;
        if (pid !== activeProfileId) return;
        if (latestSourcePath && selectedSourcePath === latestSourcePath) {
          const bySource = logsByProfileSource[pid] ?? {};
          logsByProfileSource[pid] = { ...bySource, [latestSourcePath]: [...(bySource[latestSourcePath] ?? []), line].slice(-5000) };
        }
        scheduleLogRefresh(true);
      }),
      listen<{ profile_id?: string } | string>("game://exit", ({ payload }) => {
        const pid = typeof payload === "string" ? payload : payload.profile_id ?? activeProfileId;
        if (pid === activeProfileId) scheduleLogRefresh(true);
      }),
      listen<string>("log://select-profile", ({ payload }) => { selectedProfileId = payload; sourceRefreshToken += 1; }),
    ];
    return () => { for (const pending of listeners) pending.then((dispose) => dispose()).catch(console.error); };
  });

  $effect(() => {
    const selection = selectedProfileId;
    let cancelled = false;
    invoke<Profile[]>("list_profiles").then((items) => {
      if (cancelled) return;
      profiles = items;
      if (!selection && items[0]) selectedProfileId = items[0].id;
    }).catch(console.error);
    return () => { cancelled = true; };
  });

  $effect(() => {
    const profileId = activeProfileId;
    void sourceRefreshToken;
    if (!profileId) return;
    let cancelled = false;
    invoke<LogSource[]>("list_profile_log_sources", { profileId }).then((sources) => {
      if (cancelled) return;
      sourcesByProfile[profileId] = sources;
      if (!sources.some((source) => source.source_path === selectedSourcePath)) {
        selectedSourcePath = (sources.find((source) => source.label === "latest.log") ?? sources[0])?.source_path ?? null;
      }
    }).catch(console.error);
    return () => { cancelled = true; };
  });

  $effect(() => {
    const profileId = activeProfileId;
    const sourcePath = selectedSourcePath;
    void logRefreshToken;
    if (!profileId || !sourcePath || !logSources.some((source) => source.source_path === sourcePath)) return;
    let cancelled = false;
    invoke<string[]>("read_profile_log_source", { profileId, sourcePath }).then((logLines) => {
      if (cancelled) return;
      logsByProfileSource[profileId] = { ...(logsByProfileSource[profileId] ?? {}), [sourcePath]: logLines };
    }).catch(console.error);
    return () => { cancelled = true; };
  });

  $effect(() => {
    const logLines = rawLines.slice(-5000);
    const lang = translation.language?.startsWith("ja") ? "ja" : "en";
    logDiagnosis = null;
    if (!logLines.length) return;
    let cancelled = false;
    const timer = setTimeout(() => {
      invoke<ParsedCrash>("parse_crash_log", { logLines, lang }).then((result) => {
        if (!cancelled) logDiagnosis = result;
      }).catch(() => { if (!cancelled) logDiagnosis = null; });
    }, 350);
    return () => { cancelled = true; clearTimeout(timer); };
  });

  $effect(() => {
    void lines.length;
    let cancelled = false;
    tick().then(() => {
      if (cancelled || !untrack(() => autoScroll) || !body) return;
      body.scrollTop = body.scrollHeight;
      if (scrollFrame !== undefined) cancelAnimationFrame(scrollFrame);
      scrollFrame = requestAnimationFrame(() => { if (!cancelled && body && autoScroll) body.scrollTop = body.scrollHeight; });
    });
    return () => { cancelled = true; };
  });

  function copyText(text: string) {
    navigator.clipboard.writeText(text).then(() => {
      if (destroyed) return;
      copied = true;
      clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => { copied = false; }, 1500);
    }).catch(console.error);
  }

  async function jumpToLine(lineId: number) {
    const sequence = ++jumpSequence;
    filter = "ALL";
    query = "";
    regexMode = false;
    autoScroll = false;
    await tick();
    if (destroyed || sequence !== jumpSequence) return;
    const element = rows.get(lineId);
    if (!element) return;
    element.scrollIntoView({ block: "center" });
    highlightedLine = null;
    if (jumpFrame !== undefined) cancelAnimationFrame(jumpFrame);
    jumpFrame = requestAnimationFrame(() => {
      highlightedLine = lineId;
      clearTimeout(highlightTimer);
      highlightTimer = setTimeout(() => { if (highlightedLine === lineId) highlightedLine = null; }, 1700);
    });
  }

  function startResize(event: PointerEvent) {
    event.preventDefault();
    resize = { x: event.clientX, width: sideWidth };
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }
  function moveResize(event: PointerEvent) {
    if (resize) sideWidth = Math.min(420, Math.max(220, resize.width + event.clientX - resize.x));
  }
  function resizeKey(event: KeyboardEvent) {
    if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
      event.preventDefault();
      sideWidth = Math.min(420, Math.max(220, sideWidth + (event.key === "ArrowRight" ? 10 : -10)));
    }
  }
  function focusSuspect(event: FocusEvent, text: string) {
    const bounds = (event.currentTarget as HTMLElement).getBoundingClientRect();
    suspectTip = { text, x: bounds.right, y: bounds.top };
  }
  function close() {
    const win = getCurrentWindow();
    win.close().catch(() => win.hide().catch(console.error));
  }
  onDestroy(() => {
    destroyed = true;
    clearTimeout(refreshTimer);
    clearTimeout(copiedTimer);
    clearTimeout(highlightTimer);
    if (scrollFrame !== undefined) cancelAnimationFrame(scrollFrame);
    if (jumpFrame !== undefined) cancelAnimationFrame(jumpFrame);
  });
</script>

<svelte:window onpointermove={moveResize} onpointerup={() => { resize = null; }} onpointercancel={() => { resize = null; }} />

{#snippet metric(label: string, value: number, tone = "")}
  <div class="log-metric"><span>{label}</span><b class={tone}>{value}</b></div>
{/snippet}

{#if mode === "crash"}
  <div class="tool-window crash-window">
    <div class="tool-window-titlebar"><span>Minecraft Crash</span><button onclick={close} class="log-icon-btn" title={t("common.close")}><X size={14} /></button></div>
    <GameLogCrashSummary diagnosis={logDiagnosis} {activeProfile} onCopy={() => copyText(rawLines.join("\n"))} {copied} />
  </div>
{:else}
  <div class="tool-window log-inspector-window">
    <div class="log-inspector-grid" style:grid-template-columns={`${sideWidth}px 6px minmax(0, 1fr)`}>
      <aside class="log-sidebar">
        <span class="log-side-label">{t("log.profile")}</span>
        <LogDropdown value={activeProfileId ?? ""} options={profiles.map((profile) => ({ value: profile.id, label: profile.name }))}
          ariaLabel={t("log.profile")} onChange={(value) => { selectedProfileId = value; }} />
        <span class="log-side-label mt">{t("log.sources")}</span>
        <LogDropdown value={selectedSourcePath ?? ""} options={sourceOptions} ariaLabel={t("log.sources")} onChange={(value) => { selectedSourcePath = value; }} />
        <div class="log-side-card">
          <div class="log-side-title">{t("log.signals")}</div>
          {@render metric(t("log.lines"), lines.length)}
          {@render metric(t("log.warnings"), warnCount, "warn")}
          {@render metric(t("log.errors"), errorCount, "error")}
        </div>
        <div class="log-side-card">
          <div class="log-side-title">{t("log.threads")}</div>
          {#if !threadStats.length}<span class="log-muted">{t("log.no_thread_data")}</span>
          {:else}{#each threadStats as [thread, count] (thread)}{@render metric(thread, count)}{/each}{/if}
        </div>
        <div class="log-side-card">
          <div class="log-side-title">{t("log.suspect_lines")}</div>
          {#if !suspicious.length}<span class="log-muted">{t("log.no_suspects")}</span>
          {:else}
            {#each suspicious.slice(-6) as { line, reason, source } (line.id)}
              {@const text = reason === "heuristic" ? line.raw : reason}
              <button class="suspect-link" onclick={() => jumpToLine(line.id)}
                onmouseenter={(event) => { suspectTip = { text, x: event.clientX, y: event.clientY }; }}
                onmousemove={(event) => { if (suspectTip) suspectTip = { ...suspectTip, x: event.clientX, y: event.clientY }; }}
                onmouseleave={() => { suspectTip = null; }} onfocus={(event) => focusSuspect(event, text)} onblur={() => { suspectTip = null; }}>
                <span>{line.id + 1}</span><b><small>{source}</small>{line.raw}</b>
              </button>
            {/each}
          {/if}
        </div>
        {#if suspectTip}
          <div class="log-tooltip" role="tooltip" style:left={`${Math.max(0, Math.min(window.innerWidth - 440, suspectTip.x + 14))}px`} style:top={`${Math.max(0, Math.min(window.innerHeight - 160, suspectTip.y + 14))}px`}>{suspectTip.text}</div>
        {/if}
      </aside>
      <div class="log-resizer" role="slider" tabindex="0" aria-orientation="horizontal" aria-valuenow={sideWidth} aria-valuemin={220} aria-valuemax={420}
        aria-label={t("log.resize_sidebar")} onpointerdown={startResize} onkeydown={resizeKey} title={t("log.resize_sidebar")}></div>
      <main class="log-main">
        <div class="log-window-head detached">
          <div class="log-search"><Search size={13} /><input bind:value={query} aria-label={t("log.search_placeholder")} placeholder={t("log.search_placeholder")} /></div>
          <button onclick={() => { regexMode = !regexMode; }} class={regexMode ? "log-filter-btn active" : "log-filter-btn"} aria-pressed={regexMode} title={t("log.regex_hint")}>{t("log.regex")}</button>
          {#each ["ALL", "WARN", "ERROR"] as item}
            <button onclick={() => { filter = item as Filter; }} class={filter === item ? "log-filter-btn active" : "log-filter-btn"} aria-pressed={filter === item}>{item === "ALL" ? t("log.filter_all") : item}</button>
          {/each}
          <button class={copied ? "log-icon-btn copied" : "log-icon-btn"} onclick={() => copyText(visible.map((line) => line.raw).join("\n"))} title={copied ? t("common.copied") : t("common.copy_all")}>
            {#if copied}<Check size={14} />{:else}<Copy size={14} />{/if}
          </button>
          <button class="log-icon-btn" title={t("common.delete")} onclick={() => {
            if (activeProfileId && selectedSourcePath) logsByProfileSource[activeProfileId] = { ...(logsByProfileSource[activeProfileId] ?? {}), [selectedSourcePath]: [] };
          }}><Trash2 size={13} /></button>
        </div>
        <div bind:this={body} onscroll={() => { if (body) autoScroll = body.scrollHeight - body.scrollTop - body.clientHeight < 64; }} class="log-window-body detached sb">
          {#if !visible.length}<div class="py-8 px-4 text-center text-t3 text-xs">{lines.length === 0 ? t("log.empty") : t("log.empty_filtered")}</div>
          {:else}{#each visible as line (line.id)}<GameLogRow {line} {rows} highlighted={highlightedLine === line.id} />{/each}{/if}
          <div></div>
        </div>
      </main>
    </div>
  </div>
{/if}
