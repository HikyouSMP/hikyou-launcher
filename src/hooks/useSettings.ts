import { useCallback, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { LauncherSettings } from "../types";

export const DEFAULT_SETTINGS: LauncherSettings = {
  schemaVersion: 1,
  game: {
    memoryMb: 2048,
    showSnapshots: false,
    launchAfterCreate: true,
    latestReleasePlus: true,
    snapshotPlus: false,
    latestReleasePlusMode: "balanced",
    lastVersion: null,
    lastLoader: "vanilla",
    windowW: 750,
    windowH: 470,
  },
  ui: { locale: "ja" },
  advanced: {
    enabled: false,
    debugVisible: false,
    jvmTuningMode: "smooth",
    jvmTuningModules: {
      lowLatencyGc: true,
      aggressiveJit: true,
      codeCache: true,
      g1Client: true,
    },
    jvmFlagsOverride: "",
    jdkOverride: "",
    maxConcurrentDownloads: 4,
    keepGameOpen: false,
    keepLauncherVisible: false,
    logInspectorEnabled: false,
  },
  accounts: [],
  activeAccountUuid: null,
};

export function mergeSettings(settings: Partial<LauncherSettings>): LauncherSettings {
  return {
    ...DEFAULT_SETTINGS,
    ...settings,
    game: { ...DEFAULT_SETTINGS.game, ...settings.game },
    ui: { ...DEFAULT_SETTINGS.ui, ...settings.ui },
    advanced: {
      ...DEFAULT_SETTINGS.advanced,
      ...settings.advanced,
      jvmTuningModules: {
        ...DEFAULT_SETTINGS.advanced.jvmTuningModules,
        ...settings.advanced?.jvmTuningModules,
      },
    },
    accounts: settings.accounts ?? [],
    activeAccountUuid: settings.activeAccountUuid ?? null,
  };
}

export function useSettings() {
  const [settings, setSettings] =
    useState<LauncherSettings>(DEFAULT_SETTINGS);
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  const settingsRef = useRef<LauncherSettings>(DEFAULT_SETTINGS);
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const settingsLoadedRef = useRef(true);

  const updateSettings = useCallback(
    (patch: (p: LauncherSettings) => LauncherSettings) => {
      if (!settingsLoadedRef.current) return;
      setSettings((prev) => {
        const next = patch(prev);
        settingsRef.current = next;
        if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
        saveTimerRef.current = setTimeout(
          () =>
            invoke("save_settings", { settings: next }).catch(console.error),
          500,
        );
        return next;
      });
    },
    [],
  );

  const applyLoadedSettings = useCallback((next: LauncherSettings) => {
    settingsRef.current = next;
    setSettings(next);
    settingsLoadedRef.current = true;
    setSettingsLoaded(true);
  }, []);

  const markSettingsLoaded = useCallback(() => {
    settingsLoadedRef.current = true;
    setSettingsLoaded(true);
  }, []);

  return {
    settings,
    settingsLoaded,
    setSettings,
    settingsRef,
    settingsLoadedRef,
    updateSettings,
    applyLoadedSettings,
    markSettingsLoaded,
  };
}
