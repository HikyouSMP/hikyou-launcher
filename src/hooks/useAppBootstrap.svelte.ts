import { untrack } from 'svelte';
import { resolveUpdate, type Setter, type Ref } from '../lib/state';
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import i18n from "../i18n";
import { mergeSettings } from "./useSettings.svelte";
import type { AutoMod, ActiveView, AuthTokenDebugStatus, DebugInfo, LauncherSettings, LoaderType, LoaderVersion, LaunchMetrics, SmartProfileStatus, SecureStorageDiagnostics, VersionManifest, } from "../types";
interface UseAppBootstrapParams {
    activeView: ActiveView;
    settings: LauncherSettings;
    applyLoadedSettings: (settings: LauncherSettings) => void;
    markSettingsLoaded: () => void;
    loadSavedAuth: () => void;
    refreshProfiles: () => Promise<unknown>;
    autoModsLoadedRef: Ref<boolean>;
    setAutoMods: Setter<AutoMod[]>;
    loaderType: LoaderType;
    selectedVersion: string;
    setLoaderType: Setter<LoaderType>;
    setSelectedVersion: Setter<string>;
    setManifest: Setter<VersionManifest | null>;
    setManifestLoading: Setter<boolean>;
    setFabricVersions: Setter<LoaderVersion[]>;
    setSelectedFabricVersion: Setter<string>;
    setFabricLoading: Setter<boolean>;
    setDebugInfo: Setter<DebugInfo | null>;
}
export function useAppBootstrap(params: UseAppBootstrapParams) {
    let appVersion = $state("");
    const setAppVersion: Setter<typeof appVersion> = update => untrack(() => {
        appVersion = resolveUpdate(appVersion, update);
    });
    let defaultShortcut = $state("Alt+E");
    const setDefaultShortcut: Setter<typeof defaultShortcut> = update => untrack(() => {
        defaultShortcut = resolveUpdate(defaultShortcut, update);
    });
    $effect(() => {
        return untrack(() => {
            getVersion()
                .then((version) => setAppVersion(version.split(".").slice(0, 2).join(".")))
                .catch(() => { });
        });
    });
    $effect(() => {
        return untrack(() => {
            invoke<string>("get_default_shortcut")
                .then(setDefaultShortcut)
                .catch(() => { });
        });
    });
    $effect(() => {
        void params.setDebugInfo;
        return untrack(() => {
            invoke<SecureStorageDiagnostics>("get_secure_storage_diagnostics")
                .then((storageDiagnostics) => params.setDebugInfo((prev) => ({
                ...(prev ?? { javaPath: "-", javaVersion: "-", launcherPaths: {} }),
                storageBackend: storageDiagnostics.backend,
                storageDiagnostics,
            })))
                .catch(console.error);
        });
    });
    $effect(() => {
        void params.activeView;
        void params.setDebugInfo;
        return untrack(() => {
            if (params.activeView !== "main" && params.activeView !== "debug")
                return;
            invoke<SmartProfileStatus[]>("get_smart_profile_statuses")
                .then((smartProfileStatuses) => params.setDebugInfo((prev) => ({
                ...(prev ?? { javaPath: "-", javaVersion: "-", launcherPaths: {} }),
                smartProfileStatuses,
            })))
                .catch(console.error);
        });
    });
    $effect(() => {
        void params.activeView;
        void params.settings.game.memoryMb;
        void params.settings.advanced?.jdkOverride;
        void params.settings.advanced?.enabled;
        void params.setDebugInfo;
        return untrack(() => {
            if (params.activeView !== "debug")
                return;
            const memoryMb = params.settings.game.memoryMb || 2048;
            invoke<LaunchMetrics[]>("get_launch_metric_history", { limit: 10 })
                .then((launchMetricHistory) => params.setDebugInfo((prev) => ({
                ...(prev ?? { javaPath: "-", javaVersion: "-", launcherPaths: {} }),
                launchMetricHistory,
            })))
                .catch(console.error);
            invoke<Record<string, string>>("get_launcher_paths")
                .then((paths) => params.setDebugInfo((prev) => ({
                ...(prev ?? { javaPath: "-", javaVersion: "-", launcherPaths: {} }),
                launcherPaths: paths,
            })))
                .catch(console.error);
            invoke<AuthTokenDebugStatus>("get_auth_token_debug_status")
                .then((authTokens) => params.setDebugInfo((prev) => ({
                ...(prev ?? { javaPath: "-", javaVersion: "-", launcherPaths: {} }),
                authTokens,
            })))
                .catch(console.error);
            const jdkOverride = params.settings.advanced?.enabled && params.settings.advanced.jdkOverride
                ? params.settings.advanced.jdkOverride
                : null;
            invoke<{
                found: boolean;
                java_version: number | null;
                java_full_version: string | null;
                java_dist: string | null;
                java_path: string | null;
                use_zgc: boolean;
                memory_mb: number;
            }>("get_java_debug_info", { memoryMb })
                .then((info) => {
                if (!info.found && !jdkOverride)
                    return;
                params.setDebugInfo((prev) => ({
                    launcherPaths: prev?.launcherPaths ?? {},
                    javaPath: jdkOverride ?? (prev?.lastProfileId ? prev.javaPath : info.java_path) ?? "-",
                    javaVersion: jdkOverride
                        ? i18n.t("debug.custom_jdk", { path: jdkOverride })
                        : prev?.lastProfileId ? prev.javaVersion : `Java ${info.java_full_version ?? info.java_version} - ${info.java_dist ?? "unknown"}`,
                    memoryMb: prev?.lastProfileId ? prev.memoryMb : info.memory_mb,
                    useZgc: prev?.lastProfileId ? prev.useZgc : info.use_zgc,
                    jvmArgs: prev?.jvmArgs,
                    lastProfileId: prev?.lastProfileId,
                    storageBackend: prev?.storageBackend,
                    storageDiagnostics: prev?.storageDiagnostics,
                    launchMetrics: prev?.launchMetrics,
                    launchMetricHistory: prev?.launchMetricHistory,
                    gameMilestones: prev?.gameMilestones,
                    smartProfileStatuses: prev?.smartProfileStatuses,
                    authTokens: prev?.authTokens,
                }));
            })
                .catch(console.error);
        });
    });
    $effect(() => {
        void params.applyLoadedSettings;
        void params.autoModsLoadedRef;
        void params.loadSavedAuth;
        void params.markSettingsLoaded;
        void params.refreshProfiles;
        void params.setAutoMods;
        void params.setLoaderType;
        void params.setSelectedVersion;
        return untrack(() => {
            const settingsLoad = invoke<LauncherSettings>("get_settings")
                .then((loadedSettings) => {
                const merged = mergeSettings(loadedSettings);
                invoke<AutoMod[]>("get_auto_mods")
                    .then((mods) => {
                    if (mods.length > 0) {
                        params.setAutoMods(mods);
                        params.autoModsLoadedRef.current = true;
                        return;
                    }
                    invoke<string>("detect_gpu_vendor")
                        .catch(() => "unknown")
                        .then((gpuVendor) => invoke<AutoMod[]>("init_auto_mods", { gpuVendor }))
                        .then((mods) => {
                        params.setAutoMods(mods);
                        params.autoModsLoadedRef.current = true;
                    })
                        .catch(() => {
                        params.autoModsLoadedRef.current = true;
                    });
                })
                    .catch(() => {
                    params.autoModsLoadedRef.current = true;
                });
                params.applyLoadedSettings(merged);
                if (merged.ui?.locale && merged.ui.locale !== i18n.language) {
                    i18n.changeLanguage(merged.ui.locale).catch(() => { });
                }
                if (merged.game.lastVersion)
                    params.setSelectedVersion(merged.game.lastVersion);
                if (["fabric", "quilt", "neoforge"].includes(merged.game.lastLoader)) {
                    params.setLoaderType(merged.game.lastLoader as LoaderType);
                }
            })
                .catch(() => {
                params.markSettingsLoaded();
            });
            settingsLoad.finally(() => {
                params.loadSavedAuth();
            });
            params.refreshProfiles().catch(console.error);
        });
    });
    $effect(() => {
        void params.setManifest;
        void params.setManifestLoading;
        void params.setSelectedVersion;
        return untrack(() => {
            params.setManifestLoading(true);
            invoke<VersionManifest>("get_version_manifest")
                .then((manifest) => {
                params.setManifest(manifest);
                params.setSelectedVersion(manifest.latest.release);
            })
                .catch((error) => console.error("manifest:", error))
                .finally(() => params.setManifestLoading(false));
        });
    });
    $effect(() => {
        void params.loaderType;
        void params.selectedVersion;
        void params.setFabricLoading;
        void params.setFabricVersions;
        void params.setSelectedFabricVersion;
        return untrack(() => {
            if (params.loaderType === "vanilla" || !params.selectedVersion) {
                params.setFabricLoading(false);
                return;
            }
            let cancelled = false;
            params.setFabricLoading(true);
            params.setFabricVersions([]);
            params.setSelectedFabricVersion("");
            const command = params.loaderType === "fabric"
                ? "get_fabric_versions"
                : params.loaderType === "quilt"
                    ? "get_quilt_versions"
                    : params.loaderType === "neoforge"
                        ? "get_neoforge_versions"
                        : null;
            if (!command) {
                params.setFabricLoading(false);
                return;
            }
            invoke<LoaderVersion[]>(command, { mcVersion: params.selectedVersion })
                .then((versions) => {
                if (cancelled) return;
                params.setFabricVersions(versions);
                const stable = versions.find((version) => version.stable);
                params.setSelectedFabricVersion(stable?.version ?? versions[0]?.version ?? "");
            })
                .catch(() => { if (!cancelled) params.setFabricVersions([]); })
                .finally(() => { if (!cancelled) params.setFabricLoading(false); });
            return () => { cancelled = true; };
        });
    });
    return {
        get appVersion() {
            return appVersion;
        },
        get defaultShortcut() {
            return defaultShortcut;
        }
    };
}
