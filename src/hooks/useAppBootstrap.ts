import { useEffect, useState } from "react";
import type { Dispatch, MutableRefObject, SetStateAction } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";

import i18n from "../i18n";
import { mergeSettings } from "./useSettings";
import type {
  AutoMod,
  AuthTokenDebugStatus,
  DebugInfo,
  LauncherSettings,
  LoaderType,
  LoaderVersion,
  LaunchMetrics,
  SmartProfileStatus,
  VersionManifest,
} from "../types";

type ActiveView = "main" | "settings" | "debug" | "mods" | "rec-mods";

interface UseAppBootstrapParams {
  activeView: ActiveView;
  settings: LauncherSettings;
  applyLoadedSettings: (settings: LauncherSettings) => void;
  markSettingsLoaded: () => void;
  loadSavedAuth: () => void;
  refreshProfiles: () => Promise<unknown>;

  autoModsLoadedRef: MutableRefObject<boolean>;
  setAutoMods: Dispatch<SetStateAction<AutoMod[]>>;

  loaderType: LoaderType;
  selectedVersion: string;
  setLoaderType: Dispatch<SetStateAction<LoaderType>>;
  setSelectedVersion: Dispatch<SetStateAction<string>>;
  setShowSnapshots: Dispatch<SetStateAction<boolean>>;
  setManifest: Dispatch<SetStateAction<VersionManifest | null>>;
  setManifestLoading: Dispatch<SetStateAction<boolean>>;
  setFabricVersions: Dispatch<SetStateAction<LoaderVersion[]>>;
  setSelectedFabricVersion: Dispatch<SetStateAction<string>>;
  setFabricLoading: Dispatch<SetStateAction<boolean>>;
  setDebugInfo: Dispatch<SetStateAction<DebugInfo | null>>;
}

export function useAppBootstrap({
  activeView,
  settings,
  applyLoadedSettings,
  markSettingsLoaded,
  loadSavedAuth,
  refreshProfiles,
  autoModsLoadedRef,
  setAutoMods,
  loaderType,
  selectedVersion,
  setLoaderType,
  setSelectedVersion,
  setShowSnapshots,
  setManifest,
  setManifestLoading,
  setFabricVersions,
  setSelectedFabricVersion,
  setFabricLoading,
  setDebugInfo,
}: UseAppBootstrapParams) {
  const [appVersion, setAppVersion] = useState("");
  const [defaultShortcut, setDefaultShortcut] = useState("Alt+E");

  useEffect(() => {
    getVersion()
      .then((version) => setAppVersion(version.split(".").slice(0, 2).join(".")))
      .catch(() => {});
  }, []);

  useEffect(() => {
    invoke<string>("get_default_shortcut")
      .then(setDefaultShortcut)
      .catch(() => {});
  }, []);

  useEffect(() => {
    if (activeView !== "main" && activeView !== "debug") return;
    invoke<SmartProfileStatus[]>("get_smart_profile_statuses")
      .then((smartProfileStatuses) =>
        setDebugInfo((prev) => ({
          ...(prev ?? { javaPath: "-", javaVersion: "-", launcherPaths: {} }),
          smartProfileStatuses,
        })),
      )
      .catch(console.error);
  }, [activeView, setDebugInfo]);

  useEffect(() => {
    if (activeView !== "debug") return;

    const memoryMb = settings.game.memoryMb || 2048;

    invoke<LaunchMetrics[]>("get_launch_metric_history", { limit: 10 })
      .then((launchMetricHistory) =>
        setDebugInfo((prev) => ({
          ...(prev ?? { javaPath: "-", javaVersion: "-", launcherPaths: {} }),
          launchMetricHistory,
        })),
      )
      .catch(console.error);

    invoke<Record<string, string>>("get_launcher_paths")
      .then((paths) =>
        setDebugInfo((prev) => ({
          ...(prev ?? { javaPath: "-", javaVersion: "-", launcherPaths: {} }),
          launcherPaths: paths,
        })),
      )
      .catch(console.error);

    invoke<string>("get_secure_storage_backend")
      .then((backend) =>
        setDebugInfo((prev) => ({
          ...(prev ?? { javaPath: "-", javaVersion: "-", launcherPaths: {} }),
          storageBackend: backend,
        })),
      )
      .catch(console.error);

    invoke<AuthTokenDebugStatus>("get_auth_token_debug_status")
      .then((authTokens) =>
        setDebugInfo((prev) => ({
          ...(prev ?? { javaPath: "-", javaVersion: "-", launcherPaths: {} }),
          authTokens,
        })),
      )
      .catch(console.error);

    const jdkOverride =
      settings.advanced?.enabled && settings.advanced.jdkOverride
        ? settings.advanced.jdkOverride
        : null;

    invoke<{
      found: boolean;
      java_version: number | null;
      java_dist: string | null;
      java_path: string | null;
      is_liberica_nik: boolean;
      use_zgc: boolean;
      memory_mb: number;
    }>("get_java_debug_info", { memoryMb })
      .then((info) => {
        if (!info.found && !jdkOverride) return;
        setDebugInfo((prev) => ({
          launcherPaths: prev?.launcherPaths ?? {},
          javaPath: jdkOverride ?? info.java_path ?? "-",
          javaVersion: jdkOverride
            ? i18n.t("debug.custom_jdk", { path: jdkOverride })
            : `Java ${info.java_version} - ${info.java_dist ?? "unknown"}`,
          memoryMb: info.memory_mb,
          isLiberica: info.is_liberica_nik,
          useZgc: info.use_zgc,
          jvmArgs: prev?.jvmArgs,
          lastProfileId: prev?.lastProfileId,
          storageBackend: prev?.storageBackend,
          launchMetrics: prev?.launchMetrics,
          launchMetricHistory: prev?.launchMetricHistory,
          gameMilestones: prev?.gameMilestones,
          smartProfileStatuses: prev?.smartProfileStatuses,
          authTokens: prev?.authTokens,
        }));
      })
      .catch(console.error);
  }, [
    activeView,
    settings.game.memoryMb,
    settings.advanced?.jdkOverride,
    settings.advanced?.enabled,
    setDebugInfo,
  ]);

  useEffect(() => {
    const settingsLoad = invoke<LauncherSettings>("get_settings")
      .then((loadedSettings) => {
        const merged = mergeSettings(loadedSettings);

        invoke<AutoMod[]>("get_auto_mods")
          .then((mods) => {
            if (mods.length > 0) {
              setAutoMods(mods);
              autoModsLoadedRef.current = true;
              return;
            }

            invoke<string>("detect_gpu_vendor")
              .catch(() => "unknown")
              .then((gpuVendor) =>
                invoke<AutoMod[]>("init_auto_mods", { gpuVendor }),
              )
              .then((mods) => {
                setAutoMods(mods);
                autoModsLoadedRef.current = true;
              })
              .catch(() => {
                autoModsLoadedRef.current = true;
              });
          })
          .catch(() => {
            autoModsLoadedRef.current = true;
          });

        applyLoadedSettings(merged);
        if (merged.ui?.locale && merged.ui.locale !== i18n.language) {
          i18n.changeLanguage(merged.ui.locale).catch(() => {});
        }
        if (merged.game.lastVersion) setSelectedVersion(merged.game.lastVersion);
        if (merged.game.showSnapshots) setShowSnapshots(merged.game.showSnapshots);
        if (["fabric", "quilt", "neoforge"].includes(merged.game.lastLoader)) {
          setLoaderType(merged.game.lastLoader as LoaderType);
        }
      })
      .catch(() => {
        markSettingsLoaded();
      });

    settingsLoad.finally(() => {
      loadSavedAuth();
    });
    refreshProfiles().catch(console.error);
  }, [
    applyLoadedSettings,
    autoModsLoadedRef,
    loadSavedAuth,
    markSettingsLoaded,
    refreshProfiles,
    setAutoMods,
    setLoaderType,
    setSelectedVersion,
    setShowSnapshots,
  ]);

  useEffect(() => {
    setManifestLoading(true);
    invoke<VersionManifest>("get_version_manifest")
      .then((manifest) => {
        setManifest(manifest);
        setSelectedVersion(manifest.latest.release);
      })
      .catch((error) => console.error("manifest:", error))
      .finally(() => setManifestLoading(false));
  }, [setManifest, setManifestLoading, setSelectedVersion]);

  useEffect(() => {
    if (loaderType === "vanilla" || !selectedVersion) return;

    setFabricLoading(true);
    setFabricVersions([]);
    setSelectedFabricVersion("");

    const command =
      loaderType === "fabric"
        ? "get_fabric_versions"
        : loaderType === "quilt"
          ? "get_quilt_versions"
          : loaderType === "neoforge"
            ? "get_neoforge_versions"
            : null;

    if (!command) {
      setFabricLoading(false);
      return;
    }

    invoke<LoaderVersion[]>(command, { mcVersion: selectedVersion })
      .then((versions) => {
        setFabricVersions(versions);
        const stable = versions.find((version) => version.stable);
        setSelectedFabricVersion(stable?.version ?? versions[0]?.version ?? "");
      })
      .catch(() => setFabricVersions([]))
      .finally(() => setFabricLoading(false));
  }, [
    loaderType,
    selectedVersion,
    setFabricLoading,
    setFabricVersions,
    setSelectedFabricVersion,
  ]);

  return { appVersion, defaultShortcut };
}
