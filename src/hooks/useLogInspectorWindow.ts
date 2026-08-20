import { useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Effect } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

type Args = {
  advancedEnabled?: boolean;
  logInspectorEnabled?: boolean;
  keepLauncherVisible?: boolean;
};

export function useLogInspectorWindow({
  advancedEnabled,
  logInspectorEnabled,
  keepLauncherVisible,
}: Args) {
  return useCallback(
    async (profileId?: string | null) => {
      if (!advancedEnabled || !logInspectorEnabled) return;
      await invoke("ensure_log_inspector_enabled_cmd");
      const existing = await WebviewWindow.getByLabel("game-log");
      if (existing) {
        await existing
          .destroy()
          .catch(() => existing.close().catch(console.error));
      }
      const inspector = new WebviewWindow("game-log", {
        url: "/",
        title: "Hikyou Log Inspector",
        width: 1080,
        height: 760,
        minWidth: 620,
        minHeight: 260,
        resizable: true,
        decorations: true,
        transparent: true,
        backgroundColor: "#00000000",
        windowEffects: {
          effects: [
            navigator.userAgent.includes("Macintosh")
              ? Effect.HudWindow
              : Effect.Mica,
          ],
        },
        alwaysOnTop: false,
        center: true,
      });
      inspector.once("tauri://created", async () => {
        await invoke("apply_log_window_backdrop").catch(console.error);
        if (profileId) await inspector.emit("log://select-profile", profileId);
        await inspector.setFocus().catch(console.error);
        if (!keepLauncherVisible) {
          await invoke("hide_main_window", {
            reason: "log_window_opened",
          }).catch(console.error);
        }
      });
      inspector.once("tauri://error", (event) => {
        console.error("Failed to create Log Inspector", event.payload);
      });
    },
    [advancedEnabled, keepLauncherVisible, logInspectorEnabled],
  );
}
