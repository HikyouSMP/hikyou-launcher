<script lang="ts">
import { untrack } from 'svelte';
import { resolveUpdate, type Setter, type Ref } from './lib/state';
import { css } from './lib/dom';
import { invoke } from "@tauri-apps/api/core";
import LoginModal from "./components/LoginModal.svelte";
import ErrorBoundary from "./components/ErrorBoundary.svelte";
import ProfileLibraryView from "./components/library/ProfileLibraryView.svelte";
import DebugView from "./components/DebugView.svelte";
import SettingsView from "./components/SettingsView.svelte";
import AccountPanel from "./components/AccountPanel.svelte";
import ProfileContextMenu from "./components/ProfileContextMenu.svelte";
import ProfileConfigHost from "./components/ProfileConfigHost.svelte";
import MainCommandView from "./components/MainCommandView.svelte";
import { AdvancedModeDialog, DeleteProfileDialog, LogoutConfirmDialog, OptionsCopyDialog, } from "./components/ConfirmDialogs";
import ModpackVersionDialog from "./components/ModpackVersionDialog.svelte";
import ModListRoute from "./components/ModListRoute.svelte";
import RecModsPanel from "./components/RecModsPanel.svelte";
import type { VersionManifest, LoaderVersion, LoaderType, ActiveView, LoginState, AutoMod, CrashAnalysis, DebugInfo, StoredAuth, Profile } from "./types";
import { createTranslation } from "./lib/i18n.svelte";
import { parseIntent } from "./utils/intent";
import { resolveSearchSuggestion, type SearchSuggestionAction } from "./search/suggestions";
import { loaderDisplayLabel } from "./utils/profileDisplay";
import { LATEST_RELEASE_PLUS_ID, SNAPSHOT_PLUS_ID, withSmartProfileDisplay, } from "./utils/latestReleasePlus";
import { useAuth } from "./hooks/useAuth.svelte";
import { useLaunchState } from "./hooks/useLaunchState.svelte";
import { useProfiles } from "./hooks/useProfiles.svelte";
import { useSettings as useLauncherSettings } from "./hooks/useSettings.svelte";
import { useProfileSearchModel } from "./hooks/useProfileSearchModel.svelte";
import { useCommandNavigation } from "./hooks/useCommandNavigation.svelte";
import { useGameEvents } from "./hooks/useGameEvents.svelte";
import { useAppBootstrap } from "./hooks/useAppBootstrap.svelte";
import { useAccountActions } from "./hooks/useAccountActions.svelte";
import { useProfileActions } from "./hooks/useProfileActions.svelte";
import { useSettingsInputBlur } from "./hooks/useSettingsInputBlur.svelte";
import { useCrashToastParser } from "./hooks/useCrashToastParser.svelte";
import { useWindowFocusBehavior } from "./hooks/useWindowFocusBehavior.svelte";
import { useModpackVersionDialog } from "./hooks/useModpackVersionDialog.svelte";
import { useMatchedProfiles } from "./hooks/useMatchedProfiles";
import { useLogInspectorWindow } from "./hooks/useLogInspectorWindow.svelte";
import { useProfileDeletion } from "./hooks/useProfileDeletion.svelte";
import { useEscapeHandling } from "./hooks/useEscapeHandling.svelte";
import { useAltProfileShortcuts } from "./hooks/useAltProfileShortcuts.svelte";
import { createTimeouts } from './hooks/timeouts';
const { t } = createTranslation();
const timers = createTimeouts();
type ProfilePointerEvent = MouseEvent & { currentTarget: HTMLElement };
// macOS 判定 (CSS border-radius 適用用)
const isMacOS = navigator.userAgent.includes("Macintosh");
let searchValue = $state("");
const setSearchValue: Setter<typeof searchValue> = update => untrack(() => {
    searchValue = resolveUpdate(searchValue, update);
});
let activeView = $state<ActiveView>("main");
const setActiveView: Setter<ActiveView> = update => untrack(() => {
    activeView = resolveUpdate(activeView, update);
});
useSettingsInputBlur(() => activeView === "settings");
let crashAnalyses = $state<Record<string, CrashAnalysis>>({});
const setCrashAnalyses: Setter<Record<string, CrashAnalysis>> = update => untrack(() => {
    crashAnalyses = resolveUpdate(crashAnalyses, update);
});
let crashToastProfileId = $state<string | null>(null);
const setCrashToastProfileId: Setter<string | null> = update => untrack(() => {
    crashToastProfileId = resolveUpdate(crashToastProfileId, update);
});
let crashFeedbackOpen = $state(false);
const setCrashFeedbackOpen: Setter<boolean> = update => untrack(() => {
    crashFeedbackOpen = resolveUpdate(crashFeedbackOpen, update);
});
const crashToastOpenRef = { get current() { return crashToastProfileId != null; } };
useCrashToastParser({
    get crashToastProfileId() {
        return crashToastProfileId;
    },
    get crashAnalyses() {
        return crashAnalyses;
    },
    get setCrashAnalyses() {
        return setCrashAnalyses;
    }
});
let showAccounts = $state(false);
const setShowAccounts: Setter<boolean> = update => untrack(() => {
    showAccounts = resolveUpdate(showAccounts, update);
});
const inputRef = { current: null } as Ref<HTMLInputElement | null>;
// 上級者モード（バージョンを7回連打で解除）
let advTapCount = $state(0);
const advTapTimerRef = { current: null } as Ref<ReturnType<typeof setTimeout> | null>;
let showAdvConfirm = $state(false);
const setShowAdvConfirm: Setter<boolean> = update => untrack(() => {
    showAdvConfirm = resolveUpdate(showAdvConfirm, update);
});
let loginModalOpen = $state(false);
const setLoginModalOpen: Setter<boolean> = update => untrack(() => {
    loginModalOpen = resolveUpdate(loginModalOpen, update);
});
let loginState = $state<LoginState>("idle");
const setLoginState: Setter<LoginState> = update => untrack(() => {
    loginState = resolveUpdate(loginState, update);
});
let errorMessage = $state<string | undefined>();
const setErrorMessage: Setter<(string | undefined) | undefined> = update => untrack(() => {
    errorMessage = resolveUpdate(errorMessage, update);
});
const loginModalOpenRef = { get current() { return loginModalOpen; } };
const isDraggingRef = { current: false };
const launcherSettings = useLauncherSettings();
const auth = useAuth(launcherSettings.updateSettings);
let manifest = $state<VersionManifest | null>(null);
const setManifest: Setter<VersionManifest | null> = update => untrack(() => {
    manifest = resolveUpdate(manifest, update);
});
let selectedVersion = $state("");
const setSelectedVersion: Setter<typeof selectedVersion> = update => untrack(() => {
    selectedVersion = resolveUpdate(selectedVersion, update);
});
const versionChannel = $derived(launcherSettings.settings.game.versionChannel);
const toggleVersionChannel = () => {
    if (!launcherSettings.settingsLoaded) return;
    launcherSettings.updateSettings(state => ({
        ...state,
        game: { ...state.game, versionChannel: state.game.versionChannel === "release" ? "snapshot" : "release" },
    }));
};
let manifestLoading = $state(false);
const setManifestLoading: Setter<boolean> = update => untrack(() => {
    manifestLoading = resolveUpdate(manifestLoading, update);
});
let loaderType = $state<LoaderType>("vanilla");
const setLoaderType: Setter<LoaderType> = update => untrack(() => {
    loaderType = resolveUpdate(loaderType, update);
});
let _fabricVersions = $state<LoaderVersion[]>([]);
const setFabricVersions: Setter<LoaderVersion[]> = update => untrack(() => {
    _fabricVersions = resolveUpdate(_fabricVersions, update);
});
let selectedFabricVersion = $state("");
const setSelectedFabricVersion: Setter<typeof selectedFabricVersion> = update => untrack(() => {
    selectedFabricVersion = resolveUpdate(selectedFabricVersion, update);
});
let _fabricLoading = $state(false);
const setFabricLoading: Setter<boolean> = update => untrack(() => {
    _fabricLoading = resolveUpdate(_fabricLoading, update);
});
const profileStore = useProfiles();
let activeProfileId = $state<string | null>(null);
const setActiveProfileId: Setter<string | null> = update => untrack(() => {
    activeProfileId = resolveUpdate(activeProfileId, update);
});
let creating = $state<{
    versionId: string;
    versionType?: string;
    loader: LoaderType;
    inputName: string;
} | null>(null);
const setCreating: Setter<{
    versionId: string;
    versionType?: string;
    loader: LoaderType;
    inputName: string;
} | null> = update => untrack(() => {
    creating = resolveUpdate(creating, update);
});
const launchingProfileIdRef = { current: null } as Ref<string | null>;
const createInputRef = { current: null } as Ref<HTMLInputElement | null>;
let configProfileId = $state<string | null>(null);
const setConfigProfileId: Setter<string | null> = update => untrack(() => {
    configProfileId = resolveUpdate(configProfileId, update);
});
let modsProfileId = $state<string | null>(null);
const setModsProfileId: Setter<string | null> = update => untrack(() => {
    modsProfileId = resolveUpdate(modsProfileId, update);
});
let libraryProfileId = $state<string | null>(null);
const setLibraryProfileId: Setter<string | null> = update => untrack(() => {
    libraryProfileId = resolveUpdate(libraryProfileId, update);
});
let modsReturnView = $state<"main" | "library">("main");
const setModsReturnView: Setter<"main" | "library"> = update => untrack(() => {
    modsReturnView = resolveUpdate(modsReturnView, update);
});
let optionsCopySourceId = $state<string | null>(null);
const setOptionsCopySourceId: Setter<string | null> = update => untrack(() => {
    optionsCopySourceId = resolveUpdate(optionsCopySourceId, update);
});
let optionsCopyPulse = $state<{
    profileId: string;
    kind: "pick" | "drop";
    x: number;
    y: number;
} | null>(null);
const setOptionsCopyPulse: Setter<{
    profileId: string;
    kind: "pick" | "drop";
    x: number;
    y: number;
} | null> = update => untrack(() => {
    optionsCopyPulse = resolveUpdate(optionsCopyPulse, update);
});
let optionsCopyConfirm = $state<{
    sourceId: string;
    targetId: string;
} | null>(null);
const setOptionsCopyConfirm: Setter<{
    sourceId: string;
    targetId: string;
} | null> = update => untrack(() => {
    optionsCopyConfirm = resolveUpdate(optionsCopyConfirm, update);
});
const launchStartedAtRef = { current: new Map() } as Ref<Map<string, number>>;
let searchMode = $state<"profile" | "modpack">("profile");
const setSearchMode: Setter<"profile" | "modpack"> = update => untrack(() => {
    searchMode = resolveUpdate(searchMode, update);
});
const searchSuggestion = $derived(!creating && searchMode === "profile"
    ? resolveSearchSuggestion(searchValue, versionChannel) : null);
const executeSearchSuggestion = (action: SearchSuggestionAction) => {
    if (action === "toggleVersionChannel") {
        toggleVersionChannel();
    } else if (action === "searchModpacks") {
        setSearchValue("");
        setSearchMode("modpack");
    }
    inputRef.current?.focus();
};
let modpackResults = $state<import("./types").ModSearchResult[]>([]);
const setModpackResults: Setter<import("./types").ModSearchResult[]> = update => untrack(() => {
    modpackResults = resolveUpdate(modpackResults, update);
});
const exitSearchMode = () => {
    setSearchMode("profile");
    setModpackResults([]);
    inputRef.current?.focus();
};
let modpackSearching = $state(false);
const setModpackSearching: Setter<boolean> = update => untrack(() => {
    modpackSearching = resolveUpdate(modpackSearching, update);
});
const versionDialog = useModpackVersionDialog();
let installingModpackVersion = $state<{
    projectId: string;
    versionId: string;
    title?: string;
} | null>(null);
const setInstallingModpackVersion: Setter<{
    projectId: string;
    versionId: string;
    title?: string;
} | null> = update => untrack(() => {
    installingModpackVersion = resolveUpdate(installingModpackVersion, update);
});
let logoutConfirm = $state(false);
const setLogoutConfirm: Setter<boolean> = update => untrack(() => {
    logoutConfirm = resolveUpdate(logoutConfirm, update);
});
let logoutTarget = $state<StoredAuth | null>(null);
const setLogoutTarget: Setter<StoredAuth | null> = update => untrack(() => {
    logoutTarget = resolveUpdate(logoutTarget, update);
});
let autoMods = $state<AutoMod[]>([]);
const setAutoMods: Setter<AutoMod[]> = update => untrack(() => {
    autoMods = resolveUpdate(autoMods, update);
});
const autoModsLoadedRef = { current: false };
let ctrlHeld = $state(false);
const setCtrlHeld: Setter<boolean> = update => untrack(() => {
    ctrlHeld = resolveUpdate(ctrlHeld, update);
});
// ナビゲーション方向 (forward = 右から / back = 左から)
const navDirRef = $state<Ref<"forward" | "back" | "none">>({ current: "none" });
let hoverProfileId = $state<string | null>(null);
const setHoverProfileId: Setter<string | null> = update => untrack(() => {
    hoverProfileId = resolveUpdate(hoverProfileId, update);
});
let hoverModpackIdx = $state<number | null>(null);
const setHoverModpackIdx: Setter<number | null> = update => untrack(() => {
    hoverModpackIdx = resolveUpdate(hoverModpackIdx, update);
});
let hoverVersionKey = $state<string | null>(null);
const setHoverVersionKey: Setter<string | null> = update => untrack(() => {
    hoverVersionKey = resolveUpdate(hoverVersionKey, update);
});
let navIndex = $state(-1);
const setNavIndex: Setter<typeof navIndex> = update => untrack(() => {
    navIndex = resolveUpdate(navIndex, update);
});
const navIndexRef = {
    get current() { return navIndex; },
    set current(value: number) { setNavIndex(value); },
};

const navElemsRef = { current: new Map() } as Ref<Map<string, HTMLElement>>;
const navItemsRef: Ref<string[]> = { get current() { return profileSearch.navItems; } };
let _skinImgError = $state(false);
const setSkinImgError: Setter<boolean> = update => untrack(() => {
    _skinImgError = resolveUpdate(_skinImgError, update);
});
let debugInfo = $state<DebugInfo | null>(null);
const setDebugInfo: Setter<DebugInfo | null> = update => untrack(() => {
    debugInfo = resolveUpdate(debugInfo, update);
});
$effect(() => {
    return untrack(() => {
        const clearOptionsCopySource = () => setOptionsCopySourceId(null);
        window.addEventListener("mouseup", clearOptionsCopySource);
        return () => window.removeEventListener("mouseup", clearOptionsCopySource);
    });
});
$effect(() => {
    void optionsCopyPulse;
    return untrack(() => {
        if (!optionsCopyPulse)
            return;
        const timer = window.setTimeout(() => setOptionsCopyPulse(null), 560);
        return () => window.clearTimeout(timer);
    });
});
const launchState = useLaunchState();
useGameEvents({
    get activeProfileId() {
        return activeProfileId;
    },
    get launchingProfileIdRef() {
        return launchingProfileIdRef;
    },
    get launchStartedAtRef() {
        return launchStartedAtRef;
    },
    get profileLogs() {
        return launchState.profileLogs;
    },
    get setProfileLogs() {
        return launchState.setProfileLogs;
    },
    get setProfileRunStates() {
        return launchState.setProfileRunStates;
    },
    get setDebugInfo() {
        return setDebugInfo;
    },
    get setCrashAnalyses() {
        return setCrashAnalyses;
    },
    get setCrashToastProfileId() {
        return setCrashToastProfileId;
    }
});
const bootstrap = useAppBootstrap({
    get activeView() {
        return activeView;
    },
    get settings() {
        return launcherSettings.settings;
    },
    get applyLoadedSettings() {
        return launcherSettings.applyLoadedSettings;
    },
    get markSettingsLoaded() {
        return launcherSettings.markSettingsLoaded;
    },
    get loadSavedAuth() {
        return auth.loadSavedAuth;
    },
    get refreshProfiles() {
        return profileStore.refreshProfiles;
    },
    get autoModsLoadedRef() {
        return autoModsLoadedRef;
    },
    get setAutoMods() {
        return setAutoMods;
    },
    get loaderType() {
        return loaderType;
    },
    get selectedVersion() {
        return selectedVersion;
    },
    get setLoaderType() {
        return setLoaderType;
    },
    get setSelectedVersion() {
        return setSelectedVersion;
    },
    get setManifest() {
        return setManifest;
    },
    get setManifestLoading() {
        return setManifestLoading;
    },
    get setFabricVersions() {
        return setFabricVersions;
    },
    get setSelectedFabricVersion() {
        return setSelectedFabricVersion;
    },
    get setFabricLoading() {
        return setFabricLoading;
    },
    get setDebugInfo() {
        return setDebugInfo;
    }
});
const accountActions = useAccountActions({
    get savedAuth() {
        return auth.savedAuth;
    },
    get setSavedAuth() {
        return auth.setSavedAuth;
    },
    get settingsRef() {
        return launcherSettings.settingsRef;
    },
    get updateSettings() {
        return launcherSettings.updateSettings;
    },
    get setSkinImgError() {
        return setSkinImgError;
    },
    get setLoginModalOpen() {
        return setLoginModalOpen;
    },
    get setLoginState() {
        return setLoginState;
    },
    get setErrorMessage() {
        return setErrorMessage;
    },
    get loginWindowTitle() {
        return t("auth.login_window_title");
    }
});
const profileActions = useProfileActions({
    get savedAuth() {
        return auth.savedAuth;
    },
    get handleWebviewLogin() {
        return accountActions.handleWebviewLogin;
    },
    get profiles() {
        return profileStore.profiles;
    },
    get setProfiles() {
        return profileStore.setProfiles;
    },
    get activeProfileId() {
        return activeProfileId;
    },
    get setActiveProfileId() {
        return setActiveProfileId;
    },
    get selectedVersion() {
        return selectedVersion;
    },
    get manifest() {
        return manifest;
    },
    get loaderType() {
        return loaderType;
    },
    get selectedFabricVersion() {
        return selectedFabricVersion;
    },
    get settings() {
        return launcherSettings.settings;
    },
    get profileRunStates() {
        return launchState.profileRunStates;
    },
    get setProfileRunStates() {
        return launchState.setProfileRunStates;
    },
    get setProfileLogs() {
        return launchState.setProfileLogs;
    },
    get setGameError() {
        return launchState.setGameError;
    },
    get launchingProfileIdRef() {
        return launchingProfileIdRef;
    },
    get launchStartedAtRef() {
        return launchStartedAtRef;
    },
    get crashToastProfileId() {
        return crashToastProfileId;
    },
    get setCrashToastProfileId() {
        return setCrashToastProfileId;
    },
    get setCrashAnalyses() {
        return setCrashAnalyses;
    }
});
// ── helpers ──────────────────────────────────────────────────────────────
const updateAutoMods = (mods: AutoMod[]) => {
    setAutoMods(mods);
    invoke("save_auto_mods", { mods }).catch(console.error);
};
// ── Effects ──────────────────────────────────────────────────────────────
useWindowFocusBehavior({
    get keepLauncherVisible() {
        return launcherSettings.settings.advanced?.keepLauncherVisible;
    },
    get loginModalOpenRef() {
        return loginModalOpenRef;
    },
    get crashToastOpenRef() {
        return crashToastOpenRef;
    },
    get isDraggingRef() {
        return isDraggingRef;
    },
    get inputRef() {
        return inputRef;
    }
});
const handleRefreshManifest = async () => {
    setManifestLoading(true);
    try {
        const m = await invoke<VersionManifest>("refresh_version_manifest");
        setManifest(m);
        setSelectedVersion(m.latest.release);
    }
    catch (e) {
        console.error(e);
    }
    finally {
        setManifestLoading(false);
    }
};
// ── Computed ─────────────────────────────────────────────────────────────
const intent = $derived.by(() => parseIntent(searchValue));
const latestVersion = $derived(manifest?.latest.release ?? "");
const visibleProfiles = $derived.by(() => {
    return profileStore.profiles.filter((profile) => {
        if (profile.id === LATEST_RELEASE_PLUS_ID) {
            return launcherSettings.settings.game.latestReleasePlus !== false;
        }
        if (profile.id === SNAPSHOT_PLUS_ID) {
            return launcherSettings.settings.game.snapshotPlus === true;
        }
        return true;
    })
        .map((profile) => withSmartProfileDisplay(profile, manifest));
});
const matchedProfiles = $derived(useMatchedProfiles(visibleProfiles, intent, latestVersion));
const profileSearch = useProfileSearchModel({
    get creating() {
        return creating !== null;
    },
    get intent() {
        return intent;
    },
    get manifest() {
        return manifest;
    },
    get matchedProfiles() {
        return matchedProfiles;
    },
    get modpackResults() {
        return modpackResults;
    },
    get profiles() {
        return profileStore.profiles;
    },
    get searchMode() {
        return searchMode;
    },
    get versionChannel() {
        return versionChannel;
    }
});
// プロファイル別ヘルパー
const getProfilePhase = (pid: string) => launchState.profileRunStates[pid]?.phase ?? "idle";
const getProfileDlProgress = (pid: string) => launchState.profileRunStates[pid]?.dlProgress ?? null;
const isProfileBusy = (pid: string) => {
    const ph = getProfilePhase(pid);
    return ph === "downloading" || ph === "launching" || ph === "running";
};
const isGameBusy = $derived(activeProfileId ? isProfileBusy(activeProfileId) : false);
const isDebugView: boolean = $derived(activeView === "debug");
const isSettingsView: boolean = $derived(activeView === "settings");
const isMainView: boolean = $derived(activeView === "main");
const profileDeletion = useProfileDeletion({
    get profiles() {
        return profileStore.profiles;
    },
    get setProfiles() {
        return profileStore.setProfiles;
    },
    get activeProfileId() {
        return activeProfileId;
    },
    get setActiveProfileId() {
        return setActiveProfileId;
    },
    get isProfileBusy() {
        return isProfileBusy;
    },
    get profileRunStates() {
        return launchState.profileRunStates;
    }
});
useEscapeHandling({
    onExitSearchMode: exitSearchMode,
    get creating() {
        return creating;
    },
    get setCreating() {
        return setCreating;
    },
    get configProfileId() {
        return configProfileId;
    },
    get setConfigProfileId() {
        return setConfigProfileId;
    },
    get modsProfileId() {
        return modsProfileId;
    },
    get setModsProfileId() {
        return setModsProfileId;
    },
    get modsReturnView() {
        return modsReturnView;
    },
    get activeView() {
        return activeView;
    },
    get setActiveView() {
        return setActiveView;
    },
    get showAccounts() {
        return showAccounts;
    },
    get setShowAccounts() {
        return setShowAccounts;
    },
    get searchMode() {
        return searchMode;
    },
    get searchValue() {
        return searchValue;
    },
    get setSearchValue() {
        return setSearchValue;
    },
    get versionDialogOpen() {
        return Boolean(versionDialog.versionDialogModpack);
    },
    get closeVersionDialog() {
        return versionDialog.closeModpackVersionDialog;
    },
    get logoutConfirm() {
        return logoutConfirm;
    },
    get setLogoutConfirm() {
        return setLogoutConfirm;
    },
    get deleteConfirmId() {
        return profileDeletion.deleteConfirmId;
    },
    get setDeleteConfirmId() {
        return profileDeletion.setDeleteConfirmId;
    },
    get profileCtxMenuOpen() {
        return Boolean(launchState.profileCtxMenu);
    },
    get closeProfileCtxMenu() {
        return () => launchState.setProfileCtxMenu(null);
    },
    get loginModalOpenRef() {
        return loginModalOpenRef;
    },
    get setLoginModalOpen() {
        return setLoginModalOpen;
    },
    get setLoginState() {
        return setLoginState;
    },
    get setErrorMessage() {
        return setErrorMessage;
    },
    get navDirRef() {
        return navDirRef;
    },
    get inputRef() {
        return inputRef;
    },
    get isMainView() {
        return isMainView;
    },
    get onDeleteConfirmEnter() {
        return profileDeletion.handleDeleteProfileConfirm;
    },
    get isDeleteTargetBusy() {
        return profileDeletion.deleteConfirmId ? isProfileBusy(profileDeletion.deleteConfirmId) : false;
    }
});
useAltProfileShortcuts({
    get navIndex() {
        return navIndex;
    },
    get navItemsRef() {
        return navItemsRef;
    },
    get profiles() {
        return visibleProfiles;
    },
    get setModsProfileId() {
        return setModsProfileId;
    },
    get setActiveView() {
        return setActiveView;
    },
    get navDirRef() {
        return navDirRef;
    }
});
const loaderDispLabel = loaderDisplayLabel;
const openLogInspector = useLogInspectorWindow({
    get advancedEnabled() {
        return launcherSettings.settings.advanced?.enabled;
    },
    get logInspectorEnabled() {
        return launcherSettings.settings.advanced?.logInspectorEnabled;
    },
    get keepLauncherVisible() {
        return launcherSettings.settings.advanced?.keepLauncherVisible;
    }
});
useCommandNavigation({
    onExitSearchMode: exitSearchMode,
    onToggleVersionChannel: toggleVersionChannel,
    onCompleteSearchCommand: () => {
        const suggestion = searchSuggestion;
        if (suggestion?.shortcut !== "Space") return false;
        executeSearchSuggestion(suggestion.action);
        return true;
    },
    get versionChannel() {
        return versionChannel;
    },
    get activeView() {
        return activeView;
    },
    get setActiveView() {
        return setActiveView;
    },
    get navDirRef() {
        return navDirRef;
    },
    get showAccounts() {
        return showAccounts;
    },
    get deleteConfirmId() {
        return profileDeletion.deleteConfirmId;
    },
    get logoutConfirm() {
        return logoutConfirm;
    },
    get showAdvConfirm() {
        return showAdvConfirm;
    },
    get configProfileId() {
        return configProfileId;
    },
    get loginModalOpenRef() {
        return loginModalOpenRef;
    },
    get inputRef() {
        return inputRef;
    },
    get createInputRef() {
        return createInputRef;
    },
    get isMacOS() {
        return isMacOS;
    },
    get searchMode() {
        return searchMode;
    },
    get setSearchMode() {
        return setSearchMode;
    },
    get searchValue() {
        return searchValue;
    },
    get creating() {
        return creating;
    },
    get setCreating() {
        return setCreating;
    },
    get navIndexRef() {
        return navIndexRef;
    },
    get navItemsRef() {
        return navItemsRef;
    },
    get navElemsRef() {
        return navElemsRef;
    },
    get navIndex() {
        return navIndex;
    },
    get setNavIndex() {
        return setNavIndex;
    },
    get ctrlHeld() {
        return ctrlHeld;
    },
    get setCtrlHeld() {
        return setCtrlHeld;
    },
    get setHoverProfileId() {
        return setHoverProfileId;
    },
    get setHoverModpackIdx() {
        return setHoverModpackIdx;
    },
    get setHoverVersionKey() {
        return setHoverVersionKey;
    },
    get hoverProfileId() {
        return hoverProfileId;
    },
    get hoverVersionKey() {
        return hoverVersionKey;
    },
    get profiles() {
        return profileStore.profiles;
    },
    get activeProfileId() {
        return activeProfileId;
    },
    get setActiveProfileId() {
        return setActiveProfileId;
    },
    get savedAuth() {
        return auth.savedAuth;
    },
    get manifest() {
        return manifest;
    },
    get modpackResults() {
        return modpackResults;
    },
    get setModpackResults() {
        return setModpackResults;
    },
    get setModpackSearching() {
        return setModpackSearching;
    },
    get openModpackVersionDialog() {
        return versionDialog.openModpackVersionDialog;
    },
    get versionDialogModpack() {
        return versionDialog.versionDialogModpack;
    },
    get modpackVersionsCache() {
        return versionDialog.modpackVersionsCache;
    },
    get modpackVersionIdx() {
        return versionDialog.modpackVersionIdx;
    },
    get setModpackVersionIdx() {
        return versionDialog.setModpackVersionIdx;
    },
    get installingModpackVersion() {
        return installingModpackVersion;
    },
    get setConfigProfileId() {
        return setConfigProfileId;
    },
    get setModsProfileId() {
        return setModsProfileId;
    },
    get handleDeleteProfile() {
        return profileDeletion.handleDeleteProfile;
    },
    get isProfileBusy() {
        return isProfileBusy;
    },
    get handleLaunchGame() {
        return profileActions.handleLaunchGame;
    },
    get handleWebviewLogin() {
        return accountActions.handleWebviewLogin;
    }
});
// ─────────────────────────────────────────────────────────────────────────
// Render helpers (prevent TS narrowing inside conditional JSX blocks)
// ─────────────────────────────────────────────────────────────────────────
const crashToast = $derived(crashToastProfileId
    ? crashAnalyses[crashToastProfileId]
    : null);
const canOpenLogInspector = $derived(launcherSettings.settings.advanced?.enabled && launcherSettings.settings.advanced?.logInspectorEnabled);
const dataReady = $derived(launcherSettings.settingsLoaded && auth.authLoaded);
let appContentReady = $state(false);
const setAppContentReady: Setter<boolean> = update => untrack(() => {
    appContentReady = resolveUpdate(appContentReady, update);
});
$effect(() => {
    void dataReady;
    return untrack(() => {
        if (!dataReady) {
            setAppContentReady(false);
            return;
        }
        const frame = requestAnimationFrame(() => setAppContentReady(true));
        return () => cancelAnimationFrame(frame);
    });
});
const copyCrashReport = async () => {
    if (!crashToast)
        return;
    const text = [
        `Profile: ${profileStore.profiles.find((p) => p.id === crashToast.profile_id)?.name ??
            crashToast.profile_id}`,
        `Category: ${crashToast.parsed.diagnosis.category}`,
        `Confidence: ${Math.round(crashToast.parsed.diagnosis.confidence * 100)}%`,
        "",
        crashToast.parsed.diagnosis.summary,
        "",
        "Evidence:",
        ...crashToast.parsed.diagnosis.evidence,
        "",
        "Log excerpt:",
        ...crashToast.lines.slice(-240),
    ].join("\n");
    await navigator.clipboard.writeText(text);
};
const optionsCopySource = $derived.by(() => {
    const confirmation = optionsCopyConfirm;
    return confirmation ? profileStore.profiles.find(profile => profile.id === confirmation.sourceId) : null;
});
const optionsCopyTarget = $derived.by(() => {
    const confirmation = optionsCopyConfirm;
    return confirmation ? profileStore.profiles.find(profile => profile.id === confirmation.targetId) : null;
});
const confirmOptionsCopy = async () => {
    if (!optionsCopyConfirm)
        return;
    try {
        await invoke("copy_profile_options", {
            sourceProfileId: optionsCopyConfirm.sourceId,
            targetProfileId: optionsCopyConfirm.targetId,
        });
        launchState.setGameError(undefined);
    }
    catch (error) {
        launchState.setGameError(String(error));
    }
    finally {
        setOptionsCopyConfirm(null);
    }
};
const optionsRipplePoint = (event: ProfilePointerEvent) => {
    const rect = event.currentTarget.getBoundingClientRect();
    return {
        x: ((event.clientX - rect.left) / rect.width) * 100,
        y: ((event.clientY - rect.top) / rect.height) * 100,
    };
};
</script>

<ErrorBoundary
>


<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
oncontextmenu={(e) => e.preventDefault()}
onmousedown={(e) => {
    isDraggingRef.current = true;
    const target = e.target as HTMLElement;
    const isInteractive = target.closest('button, input, textarea, select, a, [role="button"], [tabindex]');
    const isSelectableText = target.closest(".log-body, .modal-card, .crash-notice, [data-selectable]");
    if (isMainView && !isInteractive && !isSelectableText) {
        e.preventDefault(); // prevent focus from leaving input
    }
}}
class="app-main-shell overflow-hidden relative flex flex-col select-text text-t1 text-sm leading-normal font-normal"
style={css({
    borderRadius: isMacOS ? 12 : 0,
    // macOS: HudWindow の上にダーク半透明ティントを重ねる (Windows と同程度の不透明度)
    // Windows: Acrylic エフェクトの上にダーク半透明ティントを重ねる
    background: isMacOS ? "rgba(18,18,16,.68)" : "rgba(18,18,16,.74)",
    fontFamily: "'Inter','Noto Sans JP Variable','Hiragino Sans','Yu Gothic UI',system-ui,sans-serif",
})}>


<div
data-tauri-drag-region
role="presentation"
oncontextmenu={(e) => e.preventDefault()}
class="absolute top-0 left-0 right-0 h-1.5 z-9998"></div>
{#if !appContentReady}
<div
class="app-cold-start-frame"></div>
{/if}
{#if launcherSettings.error}
<div role="alert" class="p-3 text-danger text-sm" data-selectable>
    <strong>{t("common.settings_persistence_failed")}</strong>
    <p>{launcherSettings.error}</p>
    {#if launcherSettings.settingsLoadedRef.current}
    <button class="modal-btn" onclick={() => launcherSettings.retrySave()}>{t("common.retry")}</button>
    {/if}
</div>
{/if}
{#if appContentReady && showAdvConfirm}
<AdvancedModeDialog
onCancel={() => setShowAdvConfirm(false)}
onEnable={() => {
    setShowAdvConfirm(false);
    launcherSettings.updateSettings((prev) => {
        const next = {
            ...prev,
            advanced: { ...prev.advanced, enabled: true },
        };
        return next;
    });
}} />
{/if}
{#if appContentReady && profileDeletion.deleteConfirmId}
<DeleteProfileDialog
profileName={profileDeletion.deleteProfileName ?? t("profile.delete_fallback_name")}
busy={profileDeletion.pending || isProfileBusy(profileDeletion.deleteConfirmId)}
retired={profileDeletion.retired}
error={profileDeletion.error}
onRestore={profileDeletion.handleRestore}
onCancel={() => profileDeletion.setDeleteConfirmId(null)}
onConfirm={profileDeletion.handleDeleteProfileConfirm} />
{/if}
{#if appContentReady && optionsCopyConfirm && optionsCopySource && optionsCopyTarget}
<OptionsCopyDialog
sourceName={optionsCopySource.name}
targetName={optionsCopyTarget.name}
onCancel={() => setOptionsCopyConfirm(null)}
onConfirm={confirmOptionsCopy} />
{/if}
{#if appContentReady &&
    configProfileId}
{@const prof = profileStore.profiles.find((p) => p.id === configProfileId)}
{#if prof}
<ProfileConfigHost
profile={prof}
globalDefaults={{
    memoryMb: launcherSettings.settings.game.memoryMb,
    windowW: launcherSettings.settings.game.windowW ?? 854,
    windowH: launcherSettings.settings.game.windowH ?? 480,
}}
onClose={() => setConfigProfileId(null)}
onSave={profileActions.handleUpdateProfileFull}
onDelete={profileDeletion.handleDeleteProfile} />
{/if}
{/if}
{#if appContentReady && launchState.profileCtxMenu}
<ProfileContextMenu
menu={launchState.profileCtxMenu}
profiles={visibleProfiles}
savedAuth={auth.savedAuth}
isProfileBusy={isProfileBusy}
onClose={() => launchState.setProfileCtxMenu(null)}
onLaunch={(profile) => {
    setActiveProfileId(profile.id);
    profileActions.handleLaunchGame(profile);
}}
onStop={profileActions.handleStopGame}
onLogin={accountActions.handleWebviewLogin}
onEditSettings={setConfigProfileId}
onManage={(profileId) => {
    navDirRef.current = "forward";
    setLibraryProfileId(profileId);
    setActiveView("library");
}}
onDelete={profileDeletion.handleDeleteProfile} />
{/if}
{#if appContentReady && showAccounts}
<AccountPanel
authPending={loginState === "waiting"}
accounts={launcherSettings.settings.accounts}
savedAuth={auth.savedAuth}
activeAccountUuid={launcherSettings.settings.activeAccountUuid}
onClose={() => {
    setShowAccounts(false);
    setLogoutConfirm(false);
    setLogoutTarget(null);
}}
onSwitchAccount={accountActions.handleSwitchAccount}
onAddAccount={accountActions.handleWebviewLogin}
onLogoutRequest={(account) => {
    setLogoutTarget(account);
    setLogoutConfirm(true);
}} />
{/if}
{#if appContentReady && isDebugView}
<DebugView
debugInfo={debugInfo}
settings={launcherSettings.settings}
savedAuth={auth.savedAuth}
profiles={profileStore.profiles}
profileRunStates={launchState.profileRunStates}
onBack={() => {
    navDirRef.current = "forward";
    setActiveView("main");
}} />
{/if}
{#if appContentReady && isSettingsView}
<SettingsView
settings={launcherSettings.settings}
updateSettings={launcherSettings.updateSettings}
manifestLoading={manifestLoading}
defaultShortcut={bootstrap.defaultShortcut}
isMacOS={isMacOS}
navDirection={navDirRef.current}
onBack={() => {
    navDirRef.current = "forward";
    setActiveView("main");
}}
onRefreshManifest={handleRefreshManifest}
onOpenRecommendedMods={() => {
    navDirRef.current = "forward";
    setActiveView("rec-mods");
}} />
{/if}
{#if appContentReady && activeView === "library" && libraryProfileId}
{@const profile = profileStore.profiles.find((item) => item.id === libraryProfileId)}
{#if profile}
<ProfileLibraryView
profile={profile}
onBack={() => {
    navDirRef.current = "back";
    setActiveView("main");
}} />
{/if}
{/if}

{#if appContentReady &&
    activeView === "mods" &&
    modsProfileId}
{@const prof = profileStore.profiles.find((p) => p.id === modsProfileId)}
{#if prof}
{#key prof.id}
<ModListRoute
enterFrom="right">

<ProfileLibraryView
profile={prof}
initialSurface="mods"
onBack={() => {
    navDirRef.current = "forward";
    setModsProfileId(null);
    setActiveView(modsReturnView);
    setModsReturnView("main");
    timers.set(() => {
        inputRef.current?.focus();
        inputRef.current?.select();
    }, 50);
}} />

</ModListRoute>
{/key}
{/if}
{/if}
{#if appContentReady && activeView === "rec-mods"}
<ModListRoute
enterFrom="right">

<RecModsPanel
autoMods={autoMods}
onSave={updateAutoMods}
onClose={() => {
    navDirRef.current = "back";
    setActiveView("settings");
}} />

</ModListRoute>
{/if}
{#if appContentReady && isMainView}
<MainCommandView
navDirection={navDirRef.current}
header={{
    inputRef,
    suggestion: searchSuggestion,
    onSuggestionAction: executeSearchSuggestion,
    searchValue,
    searchMode,
    versionChannel,
    debugVisible: Boolean(launcherSettings.settings.advanced?.enabled && launcherSettings.settings.advanced?.debugVisible),
    activeDebug: activeView === "debug",
    canOpenLogInspector: Boolean(canOpenLogInspector),
    onSearchChange: setSearchValue,
    onOpenLogInspector: () => {
        openLogInspector(activeProfileId ??
            Object.keys(crashAnalyses)[0] ??
            Object.keys(launchState.profileLogs).find((id) => launchState.profileLogs[id]?.length > 0) ??
            profileStore.profiles[0]?.id ??
            null).catch(console.error);
    },
    onToggleDebug: () => {
        navDirRef.current = activeView === "debug" ? "forward" : "back";
        setActiveView((view) => (view === "debug" ? "main" : "debug"));
    },
    onOpenSettings: () => {
        navDirRef.current = "forward";
        setActiveView("settings");
    },
}}
crash={{
    toast: crashToast,
    profileName: crashToast
        ? profileStore.profiles.find((profile) => profile.id === crashToast.profile_id)?.name ?? "Minecraft"
        : "Minecraft",
    feedbackOpen: crashFeedbackOpen,
    onFeedbackOpenChange: setCrashFeedbackOpen,
    onClose: () => setCrashToastProfileId(null),
    onCopyReport: copyCrashReport,
}}
auth={{
    savedAuth: auth.savedAuth,
    authLoaded: auth.authLoaded,
    pending: loginState === "waiting",
    onLogin: accountActions.handleWebviewLogin,
}}
modpacks={{
    results: modpackResults,
    searching: modpackSearching,
    hoverIndex: hoverModpackIdx,
    onHoverIndexChange: setHoverModpackIdx,
    onOpenVersionDialog: versionDialog.openModpackVersionDialog,
    installingVersion: installingModpackVersion,
}}
navigation={{
    navItems: profileSearch.navItems,
    navIndex,
    navElemsRef,
    ctrlHeld,
}}
profiles={{
    matched: matchedProfiles,
    isBusy: isProfileBusy,
    getPhase: getProfilePhase,
    getDownloadProgress: getProfileDlProgress,
    hoverProfileId,
    smartStatuses: debugInfo?.smartProfileStatuses ?? [],
    onHoverProfileIdChange: setHoverProfileId,
    onOpenContextMenu: (profile: Profile, event: ProfilePointerEvent) => {
        event.preventDefault();
        setHoverProfileId(null);
        launchState.setProfileCtxMenu({
            profileId: profile.id,
            x: event.clientX,
            y: event.clientY,
        });
    },
    optionsCopySourceId,
    optionsCopyPulse,
    onOptionsCopyPick: (profile: Profile, event: ProfilePointerEvent) => {
        const point = optionsRipplePoint(event);
        setOptionsCopySourceId(profile.id);
        setOptionsCopyPulse({ profileId: profile.id, kind: "pick", ...point });
    },
    onOptionsCopyDrop: (profile: Profile, event: ProfilePointerEvent) => {
        if (!optionsCopySourceId || optionsCopySourceId === profile.id) {
            return;
        }
        const point = optionsRipplePoint(event);
        setOptionsCopyPulse({ profileId: profile.id, kind: "drop", ...point });
        setOptionsCopyConfirm({
            sourceId: optionsCopySourceId,
            targetId: profile.id,
        });
    },
    onActivate: (profile: Profile) => {
        setActiveProfileId(profile.id);
        if (auth.savedAuth)
            profileActions.handleLaunchGame(profile);
        else
            accountActions.handleWebviewLogin();
    },
    onDelete: profileDeletion.handleDeleteProfile,
    totalCount: profileStore.profiles.length,
}}
creation={{
    candidateVersions: profileSearch.candidateVersions,
    creating,
    setCreating,
    createInputRef,
    labelForLoader: loaderDispLabel,
    handleCreateProfile: async (versionId: string, loader: LoaderType, name?: string) => {
        setSearchValue("");
        return profileActions.handleCreateProfile(versionId, loader, name);
    },
    launchAfterCreate: launcherSettings.settings.game.launchAfterCreate !== false,
    loadersForCreate: profileSearch.loadersForCreate,
    hoverVersionKey,
    onHoverVersionKeyChange: setHoverVersionKey,
    intentEmpty: intent.empty,
    searchHasNoMatch: !intent.empty && !searchSuggestion &&
        matchedProfiles.length === 0 &&
        profileSearch.candidateVersions.length === 0,
    gameError: isGameBusy ? undefined : launchState.gameError,
    onClearGameError: () => launchState.setGameError(undefined),
}}
footer={{
    suggestion: searchSuggestion,
    onSuggestionAction: executeSearchSuggestion,
    appVersion: bootstrap.appVersion,
    unlockRemaining: !launcherSettings.settings.advanced?.enabled && advTapCount >= 3 ? 7 - advTapCount : null,
    accountsOpen: showAccounts,
    onVersionClick: () => {
        if (launcherSettings.settings.advanced?.enabled || showAdvConfirm) return;
        advTapCount += 1;
        if (advTapTimerRef.current)
        timers.clear(advTapTimerRef.current);
        if (advTapCount >= 7) {
            advTapCount = 0;
            if (!launcherSettings.settings.advanced?.enabled)
                setShowAdvConfirm(true);
        }
        else {
        advTapTimerRef.current = timers.set(() => {
                advTapCount = 0;
            }, 2000);
        }
    },
    onToggleAccounts: () => setShowAccounts((open) => !open),
}} />
{/if}

</div>
<LoginModal
isOpen={loginModalOpen}
state={loginState}
errorMessage={errorMessage}
onRetry={accountActions.handleWebviewLogin}
onClose={() => {
    void accountActions.cancelLogin().catch(console.error);
    setLoginModalOpen(false);
    setLoginState("idle");
    setErrorMessage(undefined);
}} />

{#if logoutConfirm}
<LogoutConfirmDialog
username={logoutTarget?.username ?? auth.savedAuth?.username}
onCancel={() => {
    setLogoutConfirm(false);
    setLogoutTarget(null);
}}
onConfirm={() => {
    if (logoutTarget)
        accountActions.handleLogoutAccount(logoutTarget);
    else
        accountActions.handleLogout();
    setLogoutConfirm(false);
    setLogoutTarget(null);
}} />
{/if}
{#if versionDialog.versionDialogModpack}
<ModpackVersionDialog
modpack={versionDialog.versionDialogModpack}
versions={versionDialog.modpackVersionsCache[versionDialog.versionDialogModpack.project_id] ?? []}
isLoading={versionDialog.loadingVersionsFor === versionDialog.versionDialogModpack.project_id}
installing={installingModpackVersion}
focusedIndex={versionDialog.modpackVersionIdx}
hoveredIndex={versionDialog.hoverModpackVersionIdx}
onHoveredIndexChange={versionDialog.setHoverModpackVersionIdx}
onClose={versionDialog.closeModpackVersionDialog}
onInstall={async (version) => {
    if (installingModpackVersion)
        return;
    const modpack = versionDialog.versionDialogModpack;
    if (!modpack) return;
    setInstallingModpackVersion({
        projectId: modpack.project_id,
        versionId: version.id,
        title: modpack.title,
    });
    versionDialog.setVersionDialogModpack(null);
    versionDialog.setHoverModpackVersionIdx(null);
    setSearchMode("profile");
    setSearchValue("");
    timers.set(() => {
        inputRef.current?.focus();
        inputRef.current?.select();
    }, 50);
    try {
        await invoke("install_modpack_as_profile", {
            projectId: modpack.project_id,
            mcVersion: version.game_versions[0] ?? "1.20.1",
            versionId: version.id,
        });
        profileStore.refreshProfiles().catch(console.error);
    }
    catch (err) {
        console.error(err);
    }
    finally {
        setInstallingModpackVersion(null);
    }
}} />
{/if}


</ErrorBoundary>
