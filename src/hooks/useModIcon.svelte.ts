import { invoke } from "@tauri-apps/api/core";

type IconData = { data_url: string; warning: string | null };
type FetchIcon = (url: string | null, projectId: string | null) => Promise<IconData>;
const inflight = new Map<string, Promise<IconData>>();
const fetchIcon: FetchIcon = (url, projectId) => {
  const key = JSON.stringify([url, projectId]);
  const existing = inflight.get(key);
  if (existing) return existing;
  if (inflight.size >= 64) return Promise.reject("icon_busy");
  const pending = invoke<IconData>("fetch_cached_icon", { url, projectId }).finally(() => inflight.delete(key));
  inflight.set(key, pending);
  return pending;
};

export function useModIcon(fetch: FetchIcon = fetchIcon) {
  let src = $state<string | null>(null);
  let status = $state<"loading" | "ready" | "missing" | "unavailable" | "unsupported" | "decode_failed">("missing");
  let cacheWarning = $state(false);
  let generation = $state(0);
  async function load(url?: string | null, projectId?: string | null) {
    const current = ++generation;
    src = null; cacheWarning = false; status = "loading";
    if (!url && !projectId) { status = "missing"; return; }
    try {
      const data = await fetch(url || null, projectId || null);
      if (current !== generation) return;
      if (data.data_url.length > 1_400_000 || !/^data:image\/(png|jpeg|webp|gif);base64,[A-Za-z0-9+/]+=*$/.test(data.data_url)) {
        status = "unsupported"; return;
      }
      src = data.data_url;
      cacheWarning = data.warning === "icon_cache_write_failed";
      status = "ready";
    } catch (error) {
      if (current !== generation) return;
      status = error === "icon_missing" ? "missing" : ["icon_invalid_image", "icon_format_unsupported", "icon_dimensions_unsupported", "icon_animated_unsupported", "icon_too_large"].includes(String(error)) ? "unsupported" : "unavailable";
    }
  }
  return {
    get src() { return src; }, get status() { return status; }, get cacheWarning() { return cacheWarning; }, get request() { return generation; },
    load,
    imageFailed(expected: string, request: number) { if (request === generation && src === expected) { src = null; status = "decode_failed"; } },
    dispose() { generation++; },
  };
}
