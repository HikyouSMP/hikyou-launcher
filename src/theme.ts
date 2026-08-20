// ─────────────────────────────────────────────────────────────────────────────
// Hikyou Design System v2 — デザイントークン
//
//   スペース/サイズ : 4の倍数 (4,8,12,16,20,24,32,40,44,48)
//   角丸            : r=6 統一（堅牢・セキュアな印象）
//   メインカラー    : #1a5427（深碧） / 表示用 #3da85c
//   エラーカラー    : #d37a6a（muted clay rose）
//   背景            : ダークグレー #262624 + grain 3.5% + frosted glass
// ─────────────────────────────────────────────────────────────────────────────

export const C = {
  // ── ベース
  bg: "#262624",
  surface: "#2e2d2b",
  surfaceHi: "#363532",
  hover: "rgba(255,255,255,.06)",
  hoverLight: "rgba(255,255,255,.03)",
  active: "rgba(255,255,255,.10)",

  // ── テキスト
  t1: "#f0efe9",   // 主テキスト
  t2: "#9b9890",   // セカンダリ
  t3: "#5a5752",   // ヒント

  // ── ボーダー
  b1: "rgba(255,255,255,.10)",
  b2: "rgba(255,255,255,.05)",

  // ── メインカラー (深碧 #1a5427)
  green: "#3da85c",
  greenDim: "#1a5427",
  greenBg: "rgba(26,84,39,.20)",
  greenBdr: "rgba(61,168,92,.22)",
  greenGlow: "rgba(61,168,92,.36)",

  // ── エラーカラー (muted clay rose)
  danger: "#d37a6a",
  dangerDim: "#7a3a32",
  dangerBg: "rgba(122,58,50,.18)",
  dangerBdr: "rgba(211,122,106,.24)",

  // ── ローダー固有カラー
  fabric: "#5a9eff",
  fabricBg: "rgba(90,158,255,.10)",
  fabricBdr: "rgba(90,158,255,.18)",
  quilt: "#9d82f0",
  quiltBg: "rgba(157,130,240,.10)",
  quiltBdr: "rgba(157,130,240,.20)",
  neoforge: "#c8922e",
  neoforgeBg: "rgba(200,146,46,.10)",
  neoforgeBdr: "rgba(200,146,46,.18)",
  forge: "#d97a3a",
  forgeBg: "rgba(217,122,58,.10)",
  forgeBdr: "rgba(217,122,58,.18)",
  vanilla: "#3da85c",
  vanillaBg: "rgba(26,84,39,.12)",
  vanillaBdr: "rgba(61,168,92,.18)",

  // ── その他
  success: "#3da85c",
  warning: "#b89030",

  // ── 角丸（全要素統一）
  r: 6,
} as const;

export type Theme = typeof C;
