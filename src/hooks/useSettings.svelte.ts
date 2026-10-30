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
    const settingsLoadedRef = { current: false };
    let error = $state<string | null>(null);
    let saveQueue = Promise.resolve();
    let saveRevision = 0;
    const persist = (next: LauncherSettings) => {
        const revision = ++saveRevision;
        saveQueue = saveQueue.then(async () => {
            try {
                await invoke("save_settings", { settings: next });
                if (revision === saveRevision) error = null;
            } catch (failure) {
                error = String(failure);
            }
        });
        return saveQueue;
    };
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
            saveTimerRef.current = setTimeout(() => { saveTimerRef.current = null; void persist(next); }, 500);
            return next;
        });
    };
    const applyLoadedSettings = (next: LauncherSettings) => {
        settingsRef.current = next;
        setSettings(next);
        settingsLoadedRef.current = true;
        setSettingsLoaded(true);
        error = null;
    };
    const markSettingsLoaded = (failure?: unknown) => {
        settingsLoadedRef.current = false;
        error = String(failure ?? "Settings could not be loaded; saving is disabled");
        setSettingsLoaded(true);
    };
    return {
        get error() { return error; },
        retrySave: () => {
            if (!settingsLoadedRef.current) return Promise.resolve();
            if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
            saveTimerRef.current = null;
            return persist(settingsRef.current);
        },
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
