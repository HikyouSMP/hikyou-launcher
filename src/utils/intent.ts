// ─────────────────────────────────────────────────────────────────────────────
// 検索インテントパーサー
//
// 設計方針:
//   · ローダー名は前方一致プレフィックスで検出 ("neof" → neoforge)
//   · スペースなし複合入力 "quilt1.21" を自動分解
//   · バージョンは「数字で始まる文字列」として広く拾う
//   · vMatch: prefix境界チェック ("1.21" → "1.21.x" ✓, "1.211" ✗)
// ─────────────────────────────────────────────────────────────────────────────

import type { LoaderType } from "../types";

export const LOADER_NAMES: LoaderType[] = ["fabric", "quilt", "neoforge", "forge", "vanilla"];

export const LOADER_ALIASES: Record<string, LoaderType> = {
  nf: "neoforge",
  fg: "forge",
  ファブリック: "fabric",
  クイルト: "quilt",
  ネオフォージ: "neoforge",
  フォージ: "forge",
  バニラ: "vanilla",
};

/** ローダー検出: 動的プレフィックス一致 + 非自明エイリアス */
export function detectLoader(token: string): LoaderType | null {
  if (token in LOADER_ALIASES) return LOADER_ALIASES[token];
  const matches = LOADER_NAMES.filter((l) => l.startsWith(token));
  return matches.length === 1 ? matches[0] : null;
}

const SNAP = new Set(["snap", "snapshot", "ss"]);
const LATE = new Set(["latest", "最新", "new", "newest"]);
// バージョン: 数字で始まりドット/数字のみ (末尾ドット許容)
const VER_RE = /^\d[\d.]*\.?$/;

export function parseIntent(q: string) {
  const raw = q.trim().toLowerCase();
  const preTokens = raw.split(/\s+/).filter(Boolean);
  const tokens: string[] = [];

  for (const t of preTokens) {
    // 複合入力分解: "quilt1.21" → ["quilt","1.21"]
    // ローダー名の最長一致 prefix を先頭から探し、直後が数字なら分割
    let split = false;
    const sortedNames = [...LOADER_NAMES].sort((a, b) => b.length - a.length);
    for (const name of sortedNames) {
      if (t.startsWith(name) && t.length > name.length && /\d/.test(t[name.length])) {
        tokens.push(name, t.slice(name.length));
        split = true;
        break;
      }
    }
    if (!split) {
      for (const [alias] of Object.entries(LOADER_ALIASES)) {
        if (t.startsWith(alias) && t.length > alias.length && /\d/.test(t[alias.length])) {
          tokens.push(alias, t.slice(alias.length));
          split = true;
          break;
        }
      }
    }
    if (!split) tokens.push(t);
  }

  let loaderHint: LoaderType | null = null;
  let verHint: string | null = null;
  let isLatest = false;
  let isSnap = false;
  const nameTokens: string[] = [];

  for (const t of tokens) {
    if (SNAP.has(t)) { isSnap = true; continue; }
    if (LATE.has(t)) { isLatest = true; continue; }
    const loader = detectLoader(t);
    if (loader !== null) { loaderHint = loader; continue; }
    if (VER_RE.test(t)) { verHint = t.replace(/\.$/, ""); continue; }
    nameTokens.push(t);
  }

  return { empty: raw === "", isLatest, isSnap, loaderHint, verHint, raw, tokens, nameTokens };
}

/**
 * バージョン一致スコア:
 *   2 = 完全一致
 *   1 = semver境界一致 ("1.21.1.x" startsWith "1.21.1.")
 *   0 = 生文字列前方一致
 *  -1 = 不一致
 */
export function vScore(id: string, hint: string): number {
  if (id === hint) return 2;
  if (id.startsWith(hint + ".")) return 1;
  if (id.startsWith(hint)) return 0;
  return -1;
}

export function vMatch(id: string, hint: string): boolean {
  return vScore(id, hint) >= 0;
}

export function normalizeSearchText(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9ぁ-んァ-ン一-龥]+/g, "");
}

export function fuzzyScore(value: string, query: string): number {
  const target = normalizeSearchText(value);
  const needle = normalizeSearchText(query);
  if (!needle) return 1;
  if (!target) return -1;
  if (target === needle) return 1000;
  if (target.startsWith(needle)) return 800 - (target.length - needle.length);
  const contiguous = target.indexOf(needle);
  if (contiguous >= 0) return 600 - contiguous - (target.length - needle.length);

  let ti = 0;
  let score = 0;
  let streak = 0;
  for (let ni = 0; ni < needle.length; ni += 1) {
    const ch = needle[ni];
    const found = target.indexOf(ch, ti);
    if (found < 0) return -1;
    if (found === ti) {
      streak += 1;
      score += 18 + streak * 4;
    } else {
      streak = 0;
      score += Math.max(1, 12 - (found - ti));
    }
    ti = found + 1;
  }
  return score - target.length * 0.25;
}
