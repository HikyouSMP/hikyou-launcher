// ─────────────────────────────────────────────────────────────────────────────
// アバターキャッシュ
//   - localStorage でセッションをまたいで永続化
//   - SHA-256 ハッシュでスキン変更を検知
//   - 同一 ID への重複 fetch を防ぐ in-flight dedup
//   - TTL 内は再フェッチしない
// ─────────────────────────────────────────────────────────────────────────────

import { useState, useEffect } from "react";

const STORAGE_KEY = "hikyou_avatar_v1";
const TTL_MS = 20 * 60 * 1000; // 20分

interface Entry {
  dataUrl: string;
  hash: string;
  ts: number;
}

type Persisted = Record<string, Entry>;

// セッション中のメモリキャッシュ (id → dataUrl)
const mem = new Map<string, string>();

function load(): Persisted {
  try { return JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}"); }
  catch { return {}; }
}

function persist(data: Persisted) {
  try { localStorage.setItem(STORAGE_KEY, JSON.stringify(data)); }
  catch {}
}

// 起動時に localStorage から読み込んでメモリに展開
const _initial = load();
for (const [id, e] of Object.entries(_initial)) mem.set(id, e.dataUrl);

// 同じ id への同時 fetch をまとめる
const inflight = new Map<string, Promise<string>>();

export function getCached(id: string): string | undefined {
  return mem.get(id);
}

export function fetchAvatar(id: string): Promise<string> {
  const flying = inflight.get(id);
  if (flying) return flying;
  const p = _fetch(id).finally(() => inflight.delete(id));
  inflight.set(id, p);
  return p;
}

async function _fetch(id: string): Promise<string> {
  const cache = load();
  const entry = cache[id];
  const now = Date.now();

  // TTL 内なら再フェッチ不要
  if (entry && now - entry.ts < TTL_MS) return entry.dataUrl;

  const resp = await fetch(`https://minotar.net/avatar/${id}/32`);
  if (!resp.ok) throw new Error(`avatar fetch failed: ${resp.status}`);

  const blob = await resp.blob();
  const ab = await blob.arrayBuffer();
  const hashBuf = await crypto.subtle.digest("SHA-256", ab);
  const hash = [...new Uint8Array(hashBuf)]
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");

  if (entry?.hash === hash) {
    // スキン変更なし — タイムスタンプだけ更新
    const updated = { ...cache, [id]: { ...entry, ts: now } };
    persist(updated);
    return entry.dataUrl;
  }

  // 新規 or スキン変更 — 画像を DataURL に変換してキャッシュ
  const dataUrl = await blobToDataUrl(blob);
  const updated = { ...cache, [id]: { dataUrl, hash, ts: now } };
  persist(updated);
  mem.set(id, dataUrl);
  return dataUrl;
}

function blobToDataUrl(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(r.result as string);
    r.onerror = reject;
    r.readAsDataURL(blob);
  });
}

// ─────────────────────────────────────────────────────────────────────────────
// useAvatarUrl — React フック
// ─────────────────────────────────────────────────────────────────────────────

export function useAvatarUrl(id: string | null | undefined): string | null {
  const [url, setUrl] = useState<string | null>(() =>
    id ? (getCached(id) ?? null) : null
  );

  useEffect(() => {
    if (!id) { setUrl(null); return; }
    const cached = getCached(id);
    if (cached) setUrl(cached); // メモリキャッシュがあれば即座に反映
    fetchAvatar(id).then(setUrl).catch(() => {}); // バックグラウンドで更新
  }, [id]);

  return url;
}
