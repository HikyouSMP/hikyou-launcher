import { useRef } from "react";
import type { Dispatch, MutableRefObject, SetStateAction } from "react";
import { invoke } from "@tauri-apps/api/core";

import type {
  CrashAnalysis,
  DownloadProgress,
  GamePhase,
  LauncherSettings,
  LoaderType,
  LoaderVersion,
  Profile,
  StoredAuth,
  VersionManifest,
} from "../types";
import { sortProfiles } from "./useProfiles";
import {
  isLatestReleasePlus,
  isSnapshotPlus,
  resolveLatestReleasePlus,
  resolveSnapshotPlus,
} from "../utils/latestReleasePlus";

type ProfileRunStates = Record<
  string,
  { phase: GamePhase; dlProgress: DownloadProgress | null }
>;

interface UpdateProfileChanges {
  name: string;
  memoryMb: number | null;
  windowW: number | null;
  windowH: number | null;
}

interface UseProfileActionsParams {
  savedAuth: StoredAuth | null;
  handleWebviewLogin: () => void | Promise<void>;
  profiles: Profile[];
  setProfiles: Dispatch<SetStateAction<Profile[]>>;
  activeProfileId: string | null;
  setActiveProfileId: Dispatch<SetStateAction<string | null>>;
  selectedVersion: string;
  manifest: VersionManifest | null;
  loaderType: LoaderType;
  selectedFabricVersion: string;
  settings: LauncherSettings;
  profileRunStates: ProfileRunStates;
  setProfileRunStates: Dispatch<SetStateAction<ProfileRunStates>>;
  setProfileLogs: Dispatch<SetStateAction<Record<string, string[]>>>;
  setGameError: Dispatch<SetStateAction<string | undefined>>;
  launchingProfileIdRef: MutableRefObject<string | null>;
  launchStartedAtRef: MutableRefObject<Map<string, number>>;
  crashToastProfileId: string | null;
  setCrashToastProfileId: Dispatch<SetStateAction<string | null>>;
  setCrashAnalyses: Dispatch<SetStateAction<Record<string, CrashAnalysis>>>;
}

export function useProfileActions({
  savedAuth,
  handleWebviewLogin,
  profiles,
  setProfiles,
  activeProfileId,
  setActiveProfileId,
  selectedVersion,
  manifest,
  loaderType,
  selectedFabricVersion,
  settings,
  profileRunStates,
  setProfileRunStates,
  setProfileLogs,
  setGameError,
  launchingProfileIdRef,
  launchStartedAtRef,
  crashToastProfileId,
  setCrashToastProfileId,
  setCrashAnalyses,
}: UseProfileActionsParams) {
  const launchLocksRef = useRef(new Set<string>());
  const createLocksRef = useRef(new Set<string>());

  const handleLaunchGame = async (profile?: Profile) => {
    if (!savedAuth) {
      await handleWebviewLogin();
      return;
    }

    const targetProfile =
      profile ??
      profiles.find((item) => item.id === activeProfileId);
    const profileKey = targetProfile?.id ?? selectedVersion ?? "unknown";
    if (launchLocksRef.current.has(profileKey)) {
      return;
    }
    const currentPhase = profileRunStates[profileKey]?.phase;
    if (
      currentPhase === "running" ||
      currentPhase === "launching" ||
      currentPhase === "downloading"
    ) {
      return;
    }
    launchLocksRef.current.add(profileKey);

    const resolvedDynamic = isLatestReleasePlus(targetProfile)
      ? await resolveLatestReleasePlus(manifest)
      : isSnapshotPlus(targetProfile)
        ? await resolveSnapshotPlus(manifest)
        : null;
    const version =
      resolvedDynamic?.mcVersion ||
      targetProfile?.mcVersion ||
      selectedVersion ||
      manifest?.latest.release ||
      "latest";
    const targetLoader =
      (resolvedDynamic?.loader as LoaderType | undefined) ||
      (targetProfile?.loader as LoaderType | undefined) ||
      loaderType;
    const loaderVersion =
      resolvedDynamic?.loaderVersion ||
      targetProfile?.loaderVersion ||
      (targetLoader !== "vanilla" ? selectedFabricVersion : null);
    const memoryMb =
      targetProfile?.memoryMb || settings.game.memoryMb || 2048;

    launchingProfileIdRef.current = profileKey;
    launchStartedAtRef.current.set(profileKey, Date.now());
    setCrashAnalyses((prev) => {
      const next = { ...prev };
      delete next[profileKey];
      return next;
    });
    if (crashToastProfileId === profileKey) setCrashToastProfileId(null);
    setProfileRunStates((prev) => ({
      ...prev,
      [profileKey]: { phase: "downloading", dlProgress: null },
    }));
    setProfileLogs((prev) => ({ ...prev, [profileKey]: [] }));
    setGameError(undefined);

    try {
      const advanced = settings.advanced;
      await invoke("launch_game", {
        version,
        memoryMb,
        loaderType: targetLoader,
        loaderVersion,
        profileId: targetProfile?.id ?? null,
        eventProfileId: profileKey,
        windowWidth: targetProfile?.windowW ?? settings.game.windowW ?? 854,
        windowHeight: targetProfile?.windowH ?? settings.game.windowH ?? 480,
        jvmFlagsOverride:
          advanced?.enabled && advanced.jvmFlagsOverride
            ? advanced.jvmFlagsOverride
            : null,
        jvmTuningMode:
          advanced?.enabled && advanced.jvmTuningMode
            ? advanced.jvmTuningMode
            : "smooth",
        jvmTuningModules:
          advanced?.enabled && advanced.jvmTuningMode === "performance_lab"
            ? Object.entries(advanced.jvmTuningModules ?? {})
                .filter(([, enabled]) => enabled)
                .map(([key]) => key)
                .join(",")
            : null,
        jdkOverride:
          advanced?.enabled && advanced.jdkOverride
            ? advanced.jdkOverride
            : null,
        maxConcurrentDownloads: advanced?.maxConcurrentDownloads ?? 16,
      });

      if (targetProfile) {
        setProfiles((prev) => {
          const updated = prev.map((item) =>
            item.id === targetProfile.id
              ? {
                  ...item,
                  lastLaunchedAt: new Date().toISOString(),
                  resolved:
                    item.kind === "smart"
                      ? {
                          mcVersion: version,
                          loader: targetLoader,
                          loaderVersion,
                          resolvedAt: new Date().toISOString(),
                        }
                      : item.resolved,
                }
              : item,
          );
          return sortProfiles(updated);
        });
      }

      setProfileRunStates((prev) => ({
        ...prev,
        [profileKey]: { phase: "running", dlProgress: null },
      }));

      if (
        !settings.advanced?.keepGameOpen &&
        !settings.advanced?.keepLauncherVisible
      ) {
        await invoke("hide_main_window", { reason: "game_launched" }).catch(
          console.error,
        );
      }
    } catch (error) {
      launchStartedAtRef.current.delete(profileKey);
      setProfileRunStates((prev) => {
        const next = { ...prev };
        delete next[profileKey];
        return next;
      });
      setGameError(String(error));
    } finally {
      if (launchingProfileIdRef.current === profileKey) {
        launchingProfileIdRef.current = null;
      }
      launchLocksRef.current.delete(profileKey);
    }
  };

  const handleCreateProfile = async (
    mcVersion: string,
    loader: LoaderType,
    name?: string,
  ): Promise<Profile | null> => {
    const createKey = `${loader}:${mcVersion}:${name?.trim() ?? ""}`;
    if (createLocksRef.current.has(createKey)) {
      return null;
    }
    createLocksRef.current.add(createKey);

    const loaderLabel =
      loader === "fabric"
        ? "Fabric"
        : loader === "quilt"
          ? "Quilt"
          : loader === "neoforge"
            ? "NeoForge"
            : loader === "forge"
              ? "Forge"
              : "Vanilla";
    const autoName = name?.trim() || `${loaderLabel} ${mcVersion}`;

    let loaderVersion: string | null = null;
    if (loader !== "vanilla") {
      const command =
        loader === "fabric"
          ? "get_fabric_versions"
          : loader === "quilt"
            ? "get_quilt_versions"
            : loader === "neoforge"
              ? "get_neoforge_versions"
              : loader === "forge"
                ? "get_forge_versions"
                : null;
      if (command) {
        try {
          const versions = await invoke<LoaderVersion[]>(command, {
            mcVersion,
          });
          const stable = versions.find((version) => version.stable);
          loaderVersion = stable?.version ?? versions[0]?.version ?? null;
        } catch (error) {
          console.warn(
            `Failed to resolve ${loaderLabel} loader version; continuing without an explicit loader version.`,
            error,
          );
        }
      }
    }

    try {
      const profile = await invoke<Profile>("create_profile", {
        name: autoName,
        mcVersion,
        loader,
        loaderVersion,
      });
      setProfiles((prev) => [profile, ...prev]);
      setActiveProfileId(profile.id);

      return profile;
    } catch (error) {
      console.error(error);
      setGameError(String(error));
      return null;
    } finally {
      createLocksRef.current.delete(createKey);
    }
  };

  const handleUpdateProfileFull = async (
    id: string,
    changes: UpdateProfileChanges,
  ) => {
    try {
      const updated = await invoke<Profile>("update_profile", {
        id,
        name: changes.name.trim() || undefined,
        memoryMb: changes.memoryMb ?? 0,
        windowW: changes.windowW ?? 0,
        windowH: changes.windowH ?? 0,
      });
      setProfiles((prev) =>
        prev.map((profile) => (profile.id === id ? updated : profile)),
      );
    } catch (error) {
      console.error(error);
    }
  };

  return { handleLaunchGame, handleCreateProfile, handleUpdateProfileFull };
}
