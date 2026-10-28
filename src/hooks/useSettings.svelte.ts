import { onDestroy, untrack } from 'svelte';
import { resolveUpdate, type Setter, type Ref } from '../lib/state';
import { invoke } from "@tauri-apps/api/core";
import type { LauncherSettings } from "../types";
export const DEFAULT_SETTINGS: LauncherSettings = {
    schemaVersion: 1,
    game: {
        javaOverrides: {},
        memoryMb: 2048,
        versionChannel: "release",
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
    const { showSnapshots, ...game } = (settings.game as Partial<LauncherSettings["game"]> & { showSnapshots?: unknown }) ?? {};
    const versionChannel = game.versionChannel === "snapshot" ||
        (game.versionChannel !== "release" && showSnapshots === true) ? "snapshot" : "release";
    return {
        ...DEFAULT_SETTINGS,
        ...settings,
        game: { ...DEFAULT_SETTINGS.game, ...game, versionChannel },
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
    let settings = $state<LauncherSettings>(DEFAULT_SETTINGS);
    const setSettings: Setter<LauncherSettings> = update => untrack(() => {
        settings = resolveUpdate(settings, update);
        settingsRef.current = settings;
    });
    let settingsLoaded = $state(false);
    const setSettingsLoaded: Setter<boolean> = update => untrack(() => {
        settingsLoaded = resolveUpdate(settingsLoaded, update);
    });
    const settingsRef = { current: DEFAULT_SETTINGS } as Ref<LauncherSettings>;
    const saveTimerRef = { current: null } as Ref<ReturnType<typeof setTimeout> | null>;
    const settingsLoadedRef = { current: true };
    onDestroy(() => {
        if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    });
    const updateSettings = (patch: (p: LauncherSettings) => LauncherSettings) => {
        if (!settingsLoadedRef.current)
            return;
        setSettings((prev) => {
            const next = patch(prev);
            settingsRef.current = next;
            if (saveTimerRef.current)
                clearTimeout(saveTimerRef.current);
            saveTimerRef.current = setTimeout(() => invoke("save_settings", { settings: next }).catch(console.error), 500);
            return next;
        });
    };
    const applyLoadedSettings = (next: LauncherSettings) => {
        settingsRef.current = next;
        setSettings(next);
        settingsLoadedRef.current = true;
        setSettingsLoaded(true);
    };
    const markSettingsLoaded = () => {
        settingsLoadedRef.current = true;
        setSettingsLoaded(true);
    };
    return {
        get settings() {
            return settings;
        },
        get settingsLoaded() {
            return settingsLoaded;
        },
        get setSettings() {
            return setSettings;
        },
        get settingsRef() {
            return settingsRef;
        },
        get settingsLoadedRef() {
            return settingsLoadedRef;
        },
        get updateSettings() {
            return updateSettings;
        },
        get applyLoadedSettings() {
            return applyLoadedSettings;
        },
        get markSettingsLoaded() {
            return markSettingsLoaded;
        }
    };
}
