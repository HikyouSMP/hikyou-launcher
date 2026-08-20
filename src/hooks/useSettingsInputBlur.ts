import { useEffect } from "react";

export function useSettingsInputBlur(enabled: boolean) {
  useEffect(() => {
    if (!enabled) return;
    const blurSettingsInput = (event: MouseEvent) => {
      const active = document.activeElement;
      if (
        !(
          active instanceof HTMLInputElement ||
          active instanceof HTMLTextAreaElement
        )
      ) {
        return;
      }
      const target = event.target;
      if (!(target instanceof HTMLElement)) return;
      if (target.closest("input, textarea, select")) return;
      active.blur();
    };
    document.addEventListener("mousedown", blurSettingsInput, true);
    return () =>
      document.removeEventListener("mousedown", blurSettingsInput, true);
  }, [enabled]);
}
