import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ChevronLeft, ChevronRight, Search, Trash2, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import i18n from "../i18n";
import type { LauncherSettings, ScreenshotItem } from "../types";

const sources = new Map<string, string>();
const pendingSources = new Map<string, Promise<string>>();

function sourceKey(item: ScreenshotItem, detail: "grid" | "preview") {
  return `${item.profileId}:${item.filename}:${item.capturedAt}:${detail}`;
}

function requestSource(item: ScreenshotItem, detail: "grid" | "preview") {
  const key = sourceKey(item, detail);
  const cached = sources.get(key);
  if (cached) return Promise.resolve(cached);
  const pending = pendingSources.get(key);
  if (pending) return pending;
  const request = invoke<string>("get_screenshot_thumbnail", {
    profileId: item.profileId,
    filename: item.filename,
    maxWidth: detail === "preview" ? 2560 : 960,
    maxHeight: detail === "preview" ? 1440 : 540,
  }).then((source) => {
    sources.set(key, source);
    pendingSources.delete(key);
    return source;
  }).catch((reason) => {
    pendingSources.delete(key);
    throw reason;
  });
  pendingSources.set(key, request);
  return request;
}

function ScreenshotImage({ item, detail = "grid" }: { item: ScreenshotItem; detail?: "grid" | "preview" }) {
  const fallback = detail === "preview" ? sources.get(sourceKey(item, "grid")) : undefined;
  const [source, setSource] = useState(() => sources.get(sourceKey(item, detail)) ?? fallback ?? null);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let active = true;
    const load = () => {
      void requestSource(item, detail).then((next) => {
        if (active) setSource(next);
      }).catch(console.error);
    };
    if (detail === "preview") {
      load();
    } else {
      const node = ref.current;
      if (!node) return;
      const observer = new IntersectionObserver((entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          load();
          observer.disconnect();
        }
      }, { rootMargin: "360px" });
      observer.observe(node);
      return () => {
        active = false;
        observer.disconnect();
      };
    }
    return () => { active = false; };
  }, [detail, item]);

  return (
    <div ref={ref} className={detail === "preview" ? "screenshot-preview-image" : "h-full w-full"}>
      {source && <img src={source} alt="" />}
    </div>
  );
}

export function ScreenshotGalleryApp() {
  const { t } = useTranslation();
  const localeHint = new URLSearchParams(window.location.search).get("locale");
  const hintedLocale = localeHint === "en" || localeHint === "ja" ? localeHint : null;
  const [localeReady, setLocaleReady] = useState(() => {
    if (hintedLocale) void i18n.changeLanguage(hintedLocale);
    return hintedLocale !== null;
  });
  const [items, setItems] = useState<ScreenshotItem[]>([]);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const [preview, setPreview] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    invoke<LauncherSettings>("get_settings")
      .then((settings) => i18n.changeLanguage(settings.ui.locale))
      .catch(console.error)
      .finally(() => setLocaleReady(true));
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void getCurrentWindow().onCloseRequested(async (event) => {
      event.preventDefault();
      await getCurrentWindow().hide();
    }).then((dispose) => { unlisten = dispose; });
    return () => unlisten?.();
  }, []);

  const reload = useCallback(async () => {
    try {
      setItems(await invoke("list_screenshots"));
      setError(null);
    } catch (reason) {
      setError(String(reason));
    }
  }, []);
  useEffect(() => void reload(), [reload]);

  const filtered = useMemo(() => {
    const value = query.trim().toLowerCase();
    if (!value) return items;
    return items.filter((item) => `${item.profileName} ${item.filename}`.toLowerCase().includes(value));
  }, [items, query]);

  const groups = useMemo(() => {
    const formatter = new Intl.DateTimeFormat(i18n.language, { year: "numeric", month: "long" });
    const result: Array<{ label: string; items: Array<{ item: ScreenshotItem; index: number }> }> = [];
    filtered.forEach((item, index) => {
      const label = formatter.format(new Date(item.capturedAt));
      const previous = result[result.length - 1];
      if (previous?.label === label) previous.items.push({ item, index });
      else result.push({ label, items: [{ item, index }] });
    });
    return result;
  }, [filtered]);

  useEffect(() => setSelected((value) => Math.min(value, Math.max(0, filtered.length - 1))), [filtered.length]);

  const moveSelection = useCallback((delta: number) => {
    setSelected((value) => Math.max(0, Math.min(filtered.length - 1, value + delta)));
  }, [filtered.length]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && preview) {
        event.preventDefault();
        setPreview(false);
        return;
      }
      if (document.activeElement === searchRef.current && !["ArrowDown", "ArrowUp", "Escape"].includes(event.key)) return;
      const columns = Math.max(1, Math.floor(window.innerWidth / 380));
      const delta = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : event.key === "ArrowDown" ? columns : event.key === "ArrowUp" ? -columns : 0;
      if (delta !== 0) {
        event.preventDefault();
        moveSelection(delta);
      } else if ((event.key === " " || event.key === "Enter") && filtered.length > 0) {
        event.preventDefault();
        setPreview(true);
      } else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f") {
        event.preventDefault();
        searchRef.current?.focus();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [filtered.length, moveSelection, preview]);

  const current = filtered[selected];
  const trashCurrent = async () => {
    if (!current) return;
    try {
      await invoke("trash_screenshot", { profileId: current.profileId, filename: current.filename });
      sources.delete(sourceKey(current, "grid"));
      sources.delete(sourceKey(current, "preview"));
      setPreview(false);
      await reload();
    } catch (reason) {
      setError(String(reason));
    }
  };

  if (!localeReady) return <div className="tool-window screenshot-gallery-window h-screen" />;

  return (
    <div className="tool-window screenshot-gallery-window flex h-screen flex-col overflow-hidden text-t1">
      <header data-tauri-drag-region className="screenshot-header">
        <div className="screenshot-title-row">
          <h1>{t("screenshots.title")}</h1>
          <span>{filtered.length}</span>
        </div>
        <label className="screenshot-search">
          <Search size={17} />
          <input ref={searchRef} value={query} onChange={(event) => setQuery(event.target.value)} placeholder={t("screenshots.search")} />
        </label>
      </header>
      {error && <div className="screenshot-error">{error}</div>}
      <main className="sb min-h-0 flex-1 overflow-y-auto">
        {filtered.length === 0 ? (
          <div className="screenshot-empty">{t("screenshots.empty")}</div>
        ) : groups.map((group) => (
          <section className="screenshot-group" key={group.label}>
            <h2>{group.label}<span>{group.items.length}</span></h2>
            <div className="screenshot-grid">
              {group.items.map(({ item, index }) => (
                <button
                  key={`${item.profileId}:${item.filename}`}
                  className={`screenshot-tile ${selected === index ? "selected" : ""}`}
                  onClick={() => setSelected(index)}
                  onDoubleClick={() => setPreview(true)}
                >
                  <ScreenshotImage item={item} />
                  <span className="screenshot-caption">
                    <strong>{item.filename.replace(/\.[^.]+$/, "")}</strong>
                    <small>{item.profileName}</small>
                  </span>
                </button>
              ))}
            </div>
          </section>
        ))}
      </main>
      {preview && current && (
        <div className="screenshot-preview" onClick={() => setPreview(false)}>
          {selected > 0 && (
            <button className="screenshot-preview-nav previous" onClick={(event) => { event.stopPropagation(); moveSelection(-1); }} title={t("common.previous")}>
              <ChevronLeft size={24} />
            </button>
          )}
          <div className="screenshot-preview-frame" onClick={(event) => event.stopPropagation()}>
            <ScreenshotImage item={current} detail="preview" />
          </div>
          {selected < filtered.length - 1 && (
            <button className="screenshot-preview-nav next" onClick={(event) => { event.stopPropagation(); moveSelection(1); }} title={t("common.next")}>
              <ChevronRight size={24} />
            </button>
          )}
          <button className="screenshot-preview-action delete" onClick={(event) => { event.stopPropagation(); void trashCurrent(); }} title={t("common.delete")}><Trash2 size={17} /></button>
          <button className="screenshot-preview-action close" onClick={(event) => { event.stopPropagation(); setPreview(false); }} title={t("common.close")}><X size={18} /></button>
        </div>
      )}
    </div>
  );
}
