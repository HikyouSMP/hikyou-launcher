import { useCallback } from "react";
import { Effect } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import i18n from "../i18n";

export function useScreenshotGallery() {
  return useCallback(async () => {
    const existing = await WebviewWindow.getByLabel("screenshots");
    if (existing) {
      await existing.show();
      await existing.setFocus();
      return;
    }
    const locale = i18n.resolvedLanguage === "en" ? "en" : "ja";
    const gallery = new WebviewWindow("screenshots", {
      url: `/?locale=${locale}`,
      title: "Hikyou Screenshots",
      width: 1040,
      height: 720,
      minWidth: 640,
      minHeight: 420,
      resizable: true,
      decorations: true,
      transparent: true,
      backgroundColor: "#00000000",
      windowEffects: {
        effects: [navigator.userAgent.includes("Macintosh") ? Effect.HudWindow : Effect.Mica],
      },
      alwaysOnTop: false,
      center: true,
    });
    gallery.once("tauri://error", (event) => {
      console.error("Failed to create Screenshot Gallery", event.payload);
    });
  }, []);
}
