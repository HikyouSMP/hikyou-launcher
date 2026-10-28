import type { Ref } from "./state";

const unitless = new Set(["opacity", "zIndex", "fontWeight", "lineHeight", "flex", "flexGrow", "flexShrink", "order", "scale", "aspectRatio"]);

export function assignRef<T extends HTMLElement>(node: T, ref: Ref<T | null> | ((node: T | null) => void)) {
  const assign = (value: T | null) => {
    if (typeof ref === "function") ref(value);
    else ref.current = value;
  };
  assign(node);
  return { destroy: () => assign(null) };
}

export function css(style: Record<string, string | number | null | undefined>): string {
  return Object.entries(style).filter(([, value]) => value != null).map(([key, value]) => {
    const name = key.startsWith("--") ? key : key.replace(/[A-Z]/g, (letter) => `-${letter.toLowerCase()}`);
    const suffix = typeof value === "number" && value !== 0 && !unitless.has(key) && !key.startsWith("--") ? "px" : "";
    return `${name}:${value}${suffix}`;
  }).join(";");
}
