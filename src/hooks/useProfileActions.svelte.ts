import { type Setter, type Ref } from '../lib/state';
import { invoke } from "@tauri-apps/api/core";
import type { CrashAnalysis, DownloadProgress, GamePhase, LauncherSettings, LoaderType, LoaderVersion, Profile, StoredAuth, VersionManifest, } from "../types";
import { sortProfiles } from "./useProfiles.svelte";
import { isSmartProfile } from "../utils/latestReleasePlus";
type ProfileRunStates = Record<string, {
    phase: GamePhase;
    dlProgress: DownloadProgress | null;
}>;
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
    setProfiles: Setter<Profile[]>;
    activeProfileId: string | null;
    setActiveProfileId: Setter<string | null>;
    selectedVersion: string;
    manifest: VersionManifest | null;
    loaderType: LoaderType;
    selectedFabricVersion: string;
    settings: LauncherSettings;
    profileRunStates: ProfileRunStates;
    setProfileRunStates: Setter<ProfileRunStates>;
    setProfileLogs: Setter<Record<string, string[]>>;
    setGameError: Setter<string | undefined>;
    launchingProfileIdRef: Ref<string | null>;
    launchStartedAtRef: Ref<Map<string, number>>;
    crashToastProfileId: string | null;
    setCrashToastProfileId: Setter<string | null>;
    setCrashAnalyses: Setter<Record<string, CrashAnalysis>>;
}
export function useProfileActions(params: UseProfileActionsParams) {
    const launchAttemptsRef = { current: new Map<string, AbortController>() };
    const createLocksRef = { current: new Set<string>() };
    const handleLaunchGame = async (profile?: Profile) => {
        if (!params.savedAuth) {
            await params.handleWebviewLogin();
            return;
        }
        const targetProfile = profile ??
            params.profiles.find((item) => item.id === params.activeProfileId);
        const profileKey = targetProfile?.id ?? params.selectedVersion ?? "unknown";
        if (launchAttemptsRef.current.has(profileKey)) {
            return;
        }
        const currentPhase = params.profileRunStates[profileKey]?.phase;
        if (currentPhase === "running" ||
            currentPhase === "launching" ||
            currentPhase === "downloading") {
            return;
        }
        const attempt = new AbortController();
        launchAttemptsRef.current.set(profileKey, attempt);
        params.launchingProfileIdRef.current = profileKey;
        params.launchStartedAtRef.current.set(profileKey, Date.now());
        params.setProfileRunStates((prev) => ({
            ...prev,
            [profileKey]: { phase: "downloading", dlProgress: null },
        }));
        try {
            const resolvedDynamic = isSmartProfile(targetProfile) ? targetProfile?.resolved : null;
            if (attempt.signal.aborted)
                throw new Error("__launch_cancelled__");
            const version = resolvedDynamic?.mcVersion ||
                targetProfile?.mcVersion || params.selectedVersion ||
                params.manifest?.latest.release ||
                "latest";
            const targetLoader = (resolvedDynamic?.loader as LoaderType | undefined) ||
                (targetProfile?.loader as LoaderType | undefined) || params.loaderType;
            const loaderVersion = resolvedDynamic?.loaderVersion ||
                targetProfile?.loaderVersion ||
                (targetLoader !== "vanilla" ? params.selectedFabricVersion : null);
            const memoryMb = targetProfile?.memoryMb || params.settings.game.memoryMb || 2048;
            params.setCrashAnalyses((prev) => {
                const next = { ...prev };
                delete next[profileKey];
                return next;
            });
            if (params.crashToastProfileId === profileKey)
                params.setCrashToastProfileId(null);
            params.setProfileLogs((prev) => ({ ...prev, [profileKey]: [] }));
            params.setGameError(undefined);
            const advanced = params.settings.advanced;
            await invoke("launch_game", {
                version,
                memoryMb,
                loaderType: targetLoader,
                loaderVersion,
                profileId: targetProfile?.id ?? null,
                eventProfileId: profileKey,
                windowWidth: targetProfile?.windowW ?? params.settings.game.windowW ?? 854,
                windowHeight: targetProfile?.windowH ?? params.settings.game.windowH ?? 480,
                jvmFlagsOverride: advanced?.enabled && advanced.jvmFlagsOverride
                    ? advanced.jvmFlagsOverride
                    : null,
                jvmTuningMode: advanced?.enabled && advanced.jvmTuningMode
                    ? advanced.jvmTuningMode
                    : "smooth",
                jvmTuningModules: advanced?.enabled && advanced.jvmTuningMode === "performance_lab"
                    ? Object.entries(advanced.jvmTuningModules ?? {})
                        .filter(([, enabled]) => enabled)
                        .map(([key]) => key)
                        .join(",")
                    : null,
                jdkOverride: advanced?.enabled && advanced.jdkOverride
                    ? advanced.jdkOverride
                    : null,
                javaOverrides: params.settings.game.javaOverrides ?? {},
                maxConcurrentDownloads: advanced?.maxConcurrentDownloads ?? 16,
            });
            if (attempt.signal.aborted) {
                await invoke("stop_game", { profileId: profileKey });
                throw new Error("__launch_cancelled__");
            }
            if (targetProfile) {
                params.setProfiles((prev) => {
                    const updated = prev.map((item) => item.id === targetProfile.id
                        ? {
                            ...item,
                            lastLaunchedAt: new Date().toISOString(),
                        }
                        : item);
                    return sortProfiles(updated);
                });
            }
            params.setProfileRunStates((prev) => prev[profileKey] ? {
                ...prev,
                [profileKey]: { phase: "running", dlProgress: null },
            } : prev);
            if (!params.settings.advanced?.keepGameOpen &&
                !params.settings.advanced?.keepLauncherVisible) {
                await invoke("hide_main_window", { reason: "game_launched" }).catch(console.error);
            }
        }
        catch (error) {
            params.launchStartedAtRef.current.delete(profileKey);
            params.setProfileRunStates((prev) => {
                const next = { ...prev };
                delete next[profileKey];
                return next;
            });
            if (!String(error).includes("__launch_cancelled__")) {
                params.setGameError(String(error));
            }
        }
        finally {
            if (params.launchingProfileIdRef.current === profileKey) {
                params.launchingProfileIdRef.current = null;
            }
            launchAttemptsRef.current.delete(profileKey);
        }
    };
    const handleStopGame = async (profileId: string) => {
        launchAttemptsRef.current.get(profileId)?.abort();
        try {
            await invoke("stop_game", { profileId });
            params.setProfileRunStates((previous) => {
                const next = { ...previous };
                delete next[profileId];
                return next;
            });
        }
        catch (error) {
            params.setGameError(String(error));
        }
    };
    const handleCreateProfile = async (mcVersion: string, loader: LoaderType, name?: string): Promise<Profile | null> => {
        const createKey = `${loader}:${mcVersion}:${name?.trim() ?? ""}`;
        if (createLocksRef.current.has(createKey)) {
            return null;
        }
        createLocksRef.current.add(createKey);
        const loaderLabel = loader === "fabric"
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
            const command = loader === "fabric"
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
                }
                catch (error) {
                    console.warn(`Failed to resolve ${loaderLabel} loader version; continuing without an explicit loader version.`, error);
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
            params.setProfiles((prev) => [profile, ...prev]);
            params.setActiveProfileId(profile.id);
            return profile;
        }
        catch (error) {
            console.error(error);
            params.setGameError(String(error));
            return null;
        }
        finally {
            createLocksRef.current.delete(createKey);
        }
    };
    const handleUpdateProfileFull = async (id: string, changes: UpdateProfileChanges) => {
        try {
            const updated = await invoke<Profile>("update_profile", {
                id,
                name: changes.name.trim() || undefined,
                memoryMb: changes.memoryMb ?? 0,
                windowW: changes.windowW ?? 0,
                windowH: changes.windowH ?? 0,
            });
            params.setProfiles((prev) => prev.map((profile) => (profile.id === id ? updated : profile)));
        }
        catch (error) {
            console.error(error);
        }
    };
    return {
        get handleLaunchGame() {
            return handleLaunchGame;
        },
        get handleStopGame() {
            return handleStopGame;
        },
        get handleCreateProfile() {
            return handleCreateProfile;
        },
        get handleUpdateProfileFull() {
            return handleUpdateProfileFull;
        }
    };
}
