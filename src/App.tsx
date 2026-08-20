import { useState, useEffect, useRef, useCallback, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LoginModal } from "./components/LoginModal";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { GameLogWindowApp } from "./components/GameLogWindowApp";
import { DebugView } from "./components/DebugView";
import { SettingsView } from "./components/SettingsView";
import { AccountPanel } from "./components/AccountPanel";
import { ProfileContextMenu } from "./components/ProfileContextMenu";
import { ProfileConfigHost } from "./components/ProfileConfigHost";
import { MainCommandView } from "./components/MainCommandView";
import {
  AdvancedModeDialog,
  DeleteProfileDialog,
  LogoutConfirmDialog,
  OptionsCopyDialog,
} from "./components/ConfirmDialogs";
import { ModpackVersionDialog } from "./components/ModpackVersionDialog";
import { ModsPanel } from "./components/ModsPanel";
import { ModListRoute } from "./components/ModListRoute";
import { RecModsPanel } from "./components/RecModsPanel";
import type {
  VersionManifest,
  LoaderVersion,
  LoaderType,
  ActiveView,
  LoginState,
  AutoMod,
  CrashAnalysis,
  DebugInfo,
  StoredAuth,
} from "./types";
import { useTranslation } from "react-i18next";
import { parseIntent } from "./utils/intent";
import { loaderDisplayLabel } from "./utils/profileDisplay";
import {
  LATEST_RELEASE_PLUS_ID,
  SNAPSHOT_PLUS_ID,
  withSmartProfileDisplay,
} from "./utils/latestReleasePlus";
import { useAuth } from "./hooks/useAuth";
import { useLaunchState } from "./hooks/useLaunchState";
import { useProfiles } from "./hooks/useProfiles";
import { useSettings as useLauncherSettings } from "./hooks/useSettings";
import { useProfileSearchModel } from "./hooks/useProfileSearchModel";
import { useCommandNavigation } from "./hooks/useCommandNavigation";
import { useGameEvents } from "./hooks/useGameEvents";
import { useAppBootstrap } from "./hooks/useAppBootstrap";
import { useAccountActions } from "./hooks/useAccountActions";
import { useProfileActions } from "./hooks/useProfileActions";
import { useSettingsInputBlur } from "./hooks/useSettingsInputBlur";
import { useCrashToastParser } from "./hooks/useCrashToastParser";
import { useWindowFocusBehavior } from "./hooks/useWindowFocusBehavior";
import { useModpackVersionDialog } from "./hooks/useModpackVersionDialog";
import { useMatchedProfiles } from "./hooks/useMatchedProfiles";
import { useLogInspectorWindow } from "./hooks/useLogInspectorWindow";
import { useProfileDeletion } from "./hooks/useProfileDeletion";
import { useEscapeHandling } from "./hooks/useEscapeHandling";
import { useAltProfileShortcuts } from "./hooks/useAltProfileShortcuts";

// ─────────────────────────────────────────────────────────────────────────────
// App
// ─────────────────────────────────────────────────────────────────────────────
export default function App() {
  const { t } = useTranslation();
  // macOS 判定 (CSS border-radius 適用用)
  const isMacOS = navigator.userAgent.includes("Macintosh");
  const currentWindowLabel = getCurrentWindow().label;
  if (currentWindowLabel === "game-log") {
    return <GameLogWindowApp />;
  }

  // UI
  const [searchValue, setSearchValue] = useState("");
  const [activeView, setActiveView] = useState<ActiveView>("main");
  useSettingsInputBlur(activeView === "settings");
  const [crashAnalyses, setCrashAnalyses] = useState<
    Record<string, CrashAnalysis>
  >({});
  const [crashToastProfileId, setCrashToastProfileId] = useState<string | null>(
    null,
  );
  const [crashFeedbackOpen, setCrashFeedbackOpen] = useState(false);
  const crashToastOpenRef = useRef(false);
  useEffect(() => {
    crashToastOpenRef.current = crashToastProfileId != null;
  }, [crashToastProfileId]);
  useCrashToastParser({
    crashToastProfileId,
    crashAnalyses,
    setCrashAnalyses,
  });
  const [showAccounts, setShowAccounts] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  // 上級者モード（バージョンを7回連打で解除）
  const advTapCountRef = useRef<number>(0);
  const advTapTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [showAdvConfirm, setShowAdvConfirm] = useState(false);

  const [loginModalOpen, setLoginModalOpen] = useState(false);
  const [loginState, setLoginState] = useState<LoginState>("idle");
  const [errorMessage, setErrorMessage] = useState<string | undefined>();
  const loginModalOpenRef = useRef(false);
  const isDraggingRef = useRef(false);
  useEffect(() => {
    loginModalOpenRef.current = loginModalOpen;
  }, [loginModalOpen]);

  // Settings
  const {
    settings,
    settingsLoaded,
    setSettings,
    settingsRef,
    updateSettings,
    applyLoadedSettings,
    markSettingsLoaded,
  } = useLauncherSettings();

  // Auth
  const { savedAuth, setSavedAuth, authLoaded, loadSavedAuth } = useAuth(updateSettings);

  // Versions
  const [manifest, setManifest] = useState<VersionManifest | null>(null);
  const [selectedVersion, setSelectedVersion] = useState("");
  const [showSnapshots, setShowSnapshots] = useState(false);
  const [manifestLoading, setManifestLoading] = useState(false);

  // Fabric
  const [loaderType, setLoaderType] = useState<LoaderType>("vanilla");
  const [_fabricVersions, setFabricVersions] = useState<LoaderVersion[]>([]);
  const [selectedFabricVersion, setSelectedFabricVersion] = useState("");
  const [_fabricLoading, setFabricLoading] = useState(false);

  // Profiles
  const { profiles, setProfiles, refreshProfiles } = useProfiles();
  const [activeProfileId, setActiveProfileId] = useState<string | null>(null);
  const [creating, setCreating] = useState<{
    versionId: string;
    versionType?: string;
    loader: LoaderType;
    inputName: string;
  } | null>(null);
  const launchingProfileIdRef = useRef<string | null>(null);
  const createInputRef = useRef<HTMLInputElement>(null);
  const [configProfileId, setConfigProfileId] = useState<string | null>(null);
  const [modsProfileId, setModsProfileId] = useState<string | null>(null);
  const [optionsCopySourceId, setOptionsCopySourceId] = useState<string | null>(null);
  const [optionsCopyPulse, setOptionsCopyPulse] = useState<{
    profileId: string;
    kind: "pick" | "drop";
    x: number;
    y: number;
  } | null>(null);
  const [optionsCopyConfirm, setOptionsCopyConfirm] = useState<{
    sourceId: string;
    targetId: string;
  } | null>(null);
  const launchStartedAtRef = useRef<Map<string, number>>(new Map());

  // Search mode (profile vs modpack)
  const [searchMode, setSearchMode] = useState<"profile" | "modpack">(
    "profile",
  );
  const [modpackResults, setModpackResults] = useState<
    import("./types").ModSearchResult[]
  >([]);
  const [modpackSearching, setModpackSearching] = useState(false);

  // Modpack version dialog
  const {
    versionDialogModpack,
    setVersionDialogModpack,
    modpackVersionsCache,
    loadingVersionsFor,
    modpackVersionIdx,
    setModpackVersionIdx,
    hoverModpackVersionIdx,
    setHoverModpackVersionIdx,
    openModpackVersionDialog,
    closeModpackVersionDialog,
  } = useModpackVersionDialog();
  const [installingModpackVersion, setInstallingModpackVersion] = useState<{
    projectId: string;
    versionId: string;
    title?: string;
  } | null>(null);

  // ログアウト確認
  const [logoutConfirm, setLogoutConfirm] = useState(false);
  const [logoutTarget, setLogoutTarget] = useState<StoredAuth | null>(null);

  // auto_mods.json
  const [autoMods, setAutoMods] = useState<AutoMod[]>([]);
  const autoModsLoadedRef = useRef(false);

  // Ctrl key tracking for shortcut badges
  const [ctrlHeld, setCtrlHeld] = useState(false);

  // ナビゲーション方向 (forward = 右から / back = 左から)
  const navDirRef = useRef<"forward" | "back" | "none">("none");

  // Independent mouse hover tracking for profile, modpack, and version items
  const [hoverProfileId, setHoverProfileId] = useState<string | null>(null);
  const [hoverModpackIdx, setHoverModpackIdx] = useState<number | null>(null);
  const [hoverVersionKey, setHoverVersionKey] = useState<string | null>(null);

  // キーボードナビゲーション
  const [navIndex, setNavIndex] = useState(-1);
  const navIndexRef = useRef(-1); // setNavIndex と同期。イベントハンドラから安全に読める
  const navElemsRef = useRef<Map<string, HTMLElement>>(new Map());
  const navItemsRef = useRef<string[]>([]);

  // アカウントアバター（スキン画像エラー追跡）
  const [_skinImgError, setSkinImgError] = useState(false);

  const [debugInfo, setDebugInfo] = useState<DebugInfo | null>(null);

  useEffect(() => {
    const clearOptionsCopySource = () => setOptionsCopySourceId(null);
    window.addEventListener("mouseup", clearOptionsCopySource);
    return () => window.removeEventListener("mouseup", clearOptionsCopySource);
  }, []);

  useEffect(() => {
    if (!optionsCopyPulse) return;
    const timer = window.setTimeout(() => setOptionsCopyPulse(null), 560);
    return () => window.clearTimeout(timer);
  }, [optionsCopyPulse]);

  // Game
  const {
    profileRunStates,
    setProfileRunStates,
    gameError,
    setGameError,
    profileLogs,
    setProfileLogs,
    profileCtxMenu,
    setProfileCtxMenu,
  } = useLaunchState();
  useGameEvents({
    activeProfileId,
    launchingProfileIdRef,
    launchStartedAtRef,
    profileLogs,
    setProfileLogs,
    setProfileRunStates,
    setDebugInfo,
    setCrashAnalyses,
    setCrashToastProfileId,
  });

  const { appVersion, defaultShortcut } = useAppBootstrap({
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
  });

  const {
    handleLogout,
    handleLogoutAccount,
    handleWebviewLogin,
    handleSwitchAccount,
  } =
    useAccountActions({
      savedAuth,
      setSavedAuth,
      settingsRef,
      updateSettings,
      setSkinImgError,
      setLoginModalOpen,
      setLoginState,
      setErrorMessage,
      loginWindowTitle: t("auth.login_window_title"),
    });

  const { handleLaunchGame, handleCreateProfile, handleUpdateProfileFull } =
    useProfileActions({
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
    });

  // ── helpers ──────────────────────────────────────────────────────────────

  const updateAutoMods = useCallback((mods: AutoMod[]) => {
    setAutoMods(mods);
    invoke("save_auto_mods", { mods }).catch(console.error);
  }, []);

  // ── Effects ──────────────────────────────────────────────────────────────

  useWindowFocusBehavior({
    keepLauncherVisible: settings.advanced?.keepLauncherVisible,
    loginModalOpenRef,
    crashToastOpenRef,
    isDraggingRef,
    inputRef,
  });

  const handleRefreshManifest = async () => {
    setManifestLoading(true);
    try {
      const m = await invoke<VersionManifest>("refresh_version_manifest");
      setManifest(m);
      setSelectedVersion(m.latest.release);
    } catch (e) {
      console.error(e);
    } finally {
      setManifestLoading(false);
    }
  };

  // ── Computed ─────────────────────────────────────────────────────────────

  const intent = useMemo(() => parseIntent(searchValue), [searchValue]);
  const latestVersion = manifest?.latest.release ?? "";
  const visibleProfiles = useMemo(
    () => {
      return profiles
        .filter((profile) => {
          if (profile.id === LATEST_RELEASE_PLUS_ID) {
            return settings.game.latestReleasePlus !== false;
          }
          if (profile.id === SNAPSHOT_PLUS_ID) {
            return settings.game.snapshotPlus === true;
          }
          return true;
        })
        .map((profile) => withSmartProfileDisplay(profile, manifest));
    },
    [manifest, profiles, settings.game.latestReleasePlus, settings.game.snapshotPlus],
  );

  const matchedProfiles = useMatchedProfiles(visibleProfiles, intent, latestVersion);

  const { candidateVersions, loadersForCreate, navItems } =
    useProfileSearchModel({
      creating: creating !== null,
      intent,
      manifest,
      matchedProfiles,
      modpackResults,
      profiles,
      searchMode,
      showSnapshots,
    });
  // navItemsRef を常に最新に保つ
  navItemsRef.current = navItems;

  // navItems が変わって未選択なら先頭を自動選択
  useEffect(() => {
    // eslint-disable-line react-hooks/exhaustive-deps
    if (
      navItems.length > 0 &&
      (navIndexRef.current < 0 || navIndexRef.current >= navItems.length)
    ) {
      setNavIndex(0);
      navIndexRef.current = 0;
    }
  }, [navItems]);

  // プロファイル別ヘルパー
  const getProfilePhase = (pid: string) =>
    profileRunStates[pid]?.phase ?? "idle";
  const getProfileDlProgress = (pid: string) =>
    profileRunStates[pid]?.dlProgress ?? null;
  const isProfileBusy = (pid: string) => {
    const ph = getProfilePhase(pid);
    return ph === "downloading" || ph === "launching" || ph === "running";
  };
  const isGameBusy = activeProfileId ? isProfileBusy(activeProfileId) : false;
  const isDebugView: boolean = activeView === "debug";
  const isSettingsView: boolean = activeView === "settings";
  const isMainView: boolean = activeView === "main";

  const handleStopGame = useCallback(
    async (profileId: string) => {
      try {
        await invoke("stop_game", { profileId });
      } catch (error) {
        setGameError(String(error));
      }
    },
    [setGameError],
  );


  const {
    deleteConfirmId,
    setDeleteConfirmId,
    handleDeleteProfile,
    handleDeleteProfileConfirm,
    deleteProfileName,
  } = useProfileDeletion({
    profiles,
    setProfiles,
    activeProfileId,
    setActiveProfileId,
    isProfileBusy,
    profileRunStates,
  });

  useEscapeHandling({
    creating,
    setCreating,
    configProfileId,
    setConfigProfileId,
    modsProfileId,
    setModsProfileId,
    activeView,
    setActiveView,
    showAccounts,
    setShowAccounts,
    searchMode,
    setSearchMode,
    searchValue,
    setSearchValue,
    versionDialogOpen: Boolean(versionDialogModpack),
    closeVersionDialog: closeModpackVersionDialog,
    logoutConfirm,
    setLogoutConfirm,
    deleteConfirmId,
    setDeleteConfirmId,
    profileCtxMenuOpen: Boolean(profileCtxMenu),
    closeProfileCtxMenu: () => setProfileCtxMenu(null),
    loginModalOpenRef,
    setLoginModalOpen,
    setLoginState,
    setErrorMessage,
    setModpackResults,
    setNavIndex,
    navIndexRef,
    navDirRef,
    inputRef,
    isMainView,
    onDeleteConfirmEnter: handleDeleteProfileConfirm,
    isDeleteTargetBusy: deleteConfirmId ? isProfileBusy(deleteConfirmId) : false,
  });

  useAltProfileShortcuts({
    navIndex,
    navItemsRef,
    profiles: visibleProfiles,
    setModsProfileId,
    setActiveView,
    navDirRef,
  });

  const loaderDispLabel = loaderDisplayLabel;

  const openLogInspector = useLogInspectorWindow({
    advancedEnabled: settings.advanced?.enabled,
    logInspectorEnabled: settings.advanced?.logInspectorEnabled,
    keepLauncherVisible: settings.advanced?.keepLauncherVisible,
  });

  useCommandNavigation({
    activeView,
    setActiveView,
    navDirRef,
    showAccounts,
    deleteConfirmId,
    logoutConfirm,
    showAdvConfirm,
    configProfileId,
    loginModalOpenRef,
    inputRef,
    createInputRef,
    isMacOS,
    searchMode,
    setSearchMode,
    searchValue,
    creating,
    setCreating,
    navIndexRef,
    navItemsRef,
    navElemsRef,
    navIndex,
    setNavIndex,
    ctrlHeld,
    setCtrlHeld,
    setHoverProfileId,
    setHoverModpackIdx,
    setHoverVersionKey,
    hoverProfileId,
    hoverVersionKey,
    profiles,
    activeProfileId,
    setActiveProfileId,
    savedAuth,
    manifest,
    modpackResults,
    setModpackResults,
    setModpackSearching,
    openModpackVersionDialog,
    versionDialogModpack,
    modpackVersionsCache,
    modpackVersionIdx,
    setModpackVersionIdx,
    installingModpackVersion,
    setConfigProfileId,
    setModsProfileId,
    handleDeleteProfile,
    isProfileBusy,
    handleLaunchGame,
    handleWebviewLogin,
  });
  // ─────────────────────────────────────────────────────────────────────────
  // Render helpers (prevent TS narrowing inside conditional JSX blocks)
  // ─────────────────────────────────────────────────────────────────────────
  const crashToast = crashToastProfileId
    ? crashAnalyses[crashToastProfileId]
    : null;
  const canOpenLogInspector =
    settings.advanced?.enabled && settings.advanced?.logInspectorEnabled;
  const dataReady = settingsLoaded && authLoaded;
  const [appContentReady, setAppContentReady] = useState(false);

  useEffect(() => {
    if (!dataReady) {
      setAppContentReady(false);
      return;
    }
    const frame = requestAnimationFrame(() => setAppContentReady(true));
    return () => cancelAnimationFrame(frame);
  }, [dataReady]);

  const copyCrashReport = useCallback(async () => {
    if (!crashToast) return;
    const text = [
      `Profile: ${
        profiles.find((p) => p.id === crashToast.profile_id)?.name ??
        crashToast.profile_id
      }`,
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
  }, [crashToast, profiles]);

  const optionsCopySource = optionsCopyConfirm
    ? profiles.find((profile) => profile.id === optionsCopyConfirm.sourceId)
    : null;
  const optionsCopyTarget = optionsCopyConfirm
    ? profiles.find((profile) => profile.id === optionsCopyConfirm.targetId)
    : null;
  const confirmOptionsCopy = useCallback(async () => {
    if (!optionsCopyConfirm) return;
    try {
      await invoke("copy_profile_options", {
        sourceProfileId: optionsCopyConfirm.sourceId,
        targetProfileId: optionsCopyConfirm.targetId,
      });
      setGameError(undefined);
    } catch (error) {
      setGameError(String(error));
    } finally {
      setOptionsCopyConfirm(null);
    }
  }, [optionsCopyConfirm, setGameError]);

  const optionsRipplePoint = (event: React.MouseEvent<HTMLElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    return {
      x: ((event.clientX - rect.left) / rect.width) * 100,
      y: ((event.clientY - rect.top) / rect.height) * 100,
    };
  };

  return (
    <ErrorBoundary>
      <>
        <div
          onContextMenu={(e) => e.preventDefault()}
          onMouseDown={(e) => {
            isDraggingRef.current = true;
            const target = e.target as HTMLElement;
            const isInteractive = target.closest(
              'button, input, textarea, select, a, [role="button"], [tabindex]',
            );
            const isSelectableText = target.closest(
              ".log-body, .modal-card, .crash-notice, [data-selectable]",
            );
            if (isMainView && !isInteractive && !isSelectableText) {
              e.preventDefault(); // prevent focus from leaving input
            }
          }}
          className="app-main-shell overflow-hidden relative flex flex-col select-text text-t1 text-sm leading-normal font-normal"
          style={{
            borderRadius: isMacOS ? 12 : 0,
            // macOS: HudWindow の上にダーク半透明ティントを重ねる (Windows と同程度の不透明度)
            // Windows: Acrylic エフェクトの上にダーク半透明ティントを重ねる
            background: isMacOS ? "rgba(18,18,16,.68)" : "rgba(18,18,16,.74)",
            fontFamily:
              "'Inter','Noto Sans JP','Hiragino Sans','Yu Gothic UI',system-ui,sans-serif",
          }}
        >
          {/* ── ドラッグストリップ: ウィンドウ上部 6px ──────────────────────────── */}
          <div
            data-tauri-drag-region
            onContextMenu={(e) => e.preventDefault()}
            className="absolute top-0 left-0 right-0 h-1.5 z-9998"
          />
          {!appContentReady && <div className="app-cold-start-frame" />}
          {appContentReady && showAdvConfirm && (
            <AdvancedModeDialog
              onCancel={() => setShowAdvConfirm(false)}
              onEnable={() => {
                setShowAdvConfirm(false);
                setSettings((prev) => {
                  const next = {
                    ...prev,
                    advanced: { ...prev.advanced, enabled: true },
                  };
                  invoke("save_settings", { settings: next }).catch(
                    console.error,
                  );
                  return next;
                });
              }}
            />
          )}

          {appContentReady && deleteConfirmId && (
            <DeleteProfileDialog
              profileName={
                deleteProfileName ?? t("profile.delete_fallback_name")
              }
              busy={isProfileBusy(deleteConfirmId)}
              onCancel={() => setDeleteConfirmId(null)}
              onConfirm={handleDeleteProfileConfirm}
            />
          )}

          {appContentReady && optionsCopyConfirm && optionsCopySource && optionsCopyTarget && (
            <OptionsCopyDialog
              sourceName={optionsCopySource.name}
              targetName={optionsCopyTarget.name}
              onCancel={() => setOptionsCopyConfirm(null)}
              onConfirm={confirmOptionsCopy}
            />
          )}

          {appContentReady &&
            configProfileId &&
            (() => {
              const prof = profiles.find((p) => p.id === configProfileId);
              if (!prof) return null;
              return (
                <ProfileConfigHost
                  profile={prof}
                  globalDefaults={{
                    memoryMb: settings.game.memoryMb,
                    windowW: settings.game.windowW ?? 854,
                    windowH: settings.game.windowH ?? 480,
                  }}
                  onClose={() => setConfigProfileId(null)}
                  onSave={handleUpdateProfileFull}
                  onDelete={handleDeleteProfile}
                />
              );
            })()}
          {appContentReady && profileCtxMenu && (
            <ProfileContextMenu
              menu={profileCtxMenu}
              profiles={visibleProfiles}
              savedAuth={savedAuth}
              isProfileBusy={isProfileBusy}
              onClose={() => setProfileCtxMenu(null)}
              onLaunch={(profile) => {
                setActiveProfileId(profile.id);
                handleLaunchGame(profile);
              }}
              onStop={handleStopGame}
              onLogin={handleWebviewLogin}
              onEditSettings={setConfigProfileId}
              onManageMods={(profileId) => {
                navDirRef.current = "forward";
                setModsProfileId(profileId);
                setActiveView("mods");
              }}
              onDelete={handleDeleteProfile}
            />
          )}
          {appContentReady && showAccounts && (
            <AccountPanel
              accounts={settings.accounts}
              savedAuth={savedAuth}
              activeAccountUuid={settings.activeAccountUuid}
              onClose={() => {
                setShowAccounts(false);
                setLogoutConfirm(false);
                setLogoutTarget(null);
              }}
              onSwitchAccount={handleSwitchAccount}
              onAddAccount={handleWebviewLogin}
              onLogoutRequest={(account) => {
                setLogoutTarget(account);
                setLogoutConfirm(true);
              }}
            />
          )}
          {appContentReady && isDebugView && (
            <DebugView
              debugInfo={debugInfo}
              settings={settings}
              savedAuth={savedAuth}
              profiles={profiles}
              profileRunStates={profileRunStates}
              onBack={() => {
                navDirRef.current = "forward";
                setActiveView("main");
              }}
            />
          )}
          {appContentReady && isSettingsView && (
            <SettingsView
              settings={settings}
              showSnapshots={showSnapshots}
              setShowSnapshots={setShowSnapshots}
              updateSettings={updateSettings}
              manifestLoading={manifestLoading}
              defaultShortcut={defaultShortcut}
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
              }}
            />
          )}
          {/* ══════════════════════════════════════════════════════════════════
          Mod 管理ビュー
      ══════════════════════════════════════════════════════════════════ */}
          {appContentReady &&
            activeView === "mods" &&
            modsProfileId &&
            (() => {
              const prof = profiles.find((p) => p.id === modsProfileId);
              if (!prof) return null;
              return (
                <ModListRoute
                  key={`mods:${prof.id}`}
                  enterFrom="right"
                >
                  <ModsPanel
                    profileId={prof.id}
                    profileName={prof.name}
                    mcVersion={prof.mcVersion}
                    loader={prof.loader}
                    onClose={() => {
                      navDirRef.current = "forward";
                      setModsProfileId(null);
                      setActiveView("main");
                      setTimeout(() => {
                        inputRef.current?.focus();
                        inputRef.current?.select();
                      }, 50);
                    }}
                  />
                </ModListRoute>
              );
            })()}

          {appContentReady && activeView === "rec-mods" && (
            <ModListRoute key="rec-mods" enterFrom="right">
              <RecModsPanel
                autoMods={autoMods}
                onSave={updateAutoMods}
                onClose={() => {
                  navDirRef.current = "back";
                  setActiveView("settings");
                }}
              />
            </ModListRoute>
          )}

          {appContentReady && isMainView && (
            <MainCommandView
              navDirection={navDirRef.current}
              header={{
                inputRef,
                searchValue,
                searchMode,
                debugVisible: Boolean(settings.advanced?.enabled && settings.advanced?.debugVisible),
                activeDebug: activeView === "debug",
                canOpenLogInspector: Boolean(canOpenLogInspector),
                onSearchChange: (value) => {
                  setSearchValue(value);
                  setNavIndex(-1);
                },
                onOpenLogInspector: () => {
                  openLogInspector(
                    activeProfileId ??
                      Object.keys(crashAnalyses)[0] ??
                      Object.keys(profileLogs).find((id) => profileLogs[id]?.length > 0) ??
                      profiles[0]?.id ??
                      null,
                  ).catch(console.error);
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
                  ? profiles.find((profile) => profile.id === crashToast.profile_id)?.name ?? "Minecraft"
                  : "Minecraft",
                feedbackOpen: crashFeedbackOpen,
                onFeedbackOpenChange: setCrashFeedbackOpen,
                onClose: () => setCrashToastProfileId(null),
                onCopyReport: copyCrashReport,
              }}
              auth={{
                savedAuth,
                authLoaded,
                onLogin: handleWebviewLogin,
              }}
              modpacks={{
                results: modpackResults,
                searching: modpackSearching,
                hoverIndex: hoverModpackIdx,
                onHoverIndexChange: setHoverModpackIdx,
                onOpenVersionDialog: openModpackVersionDialog,
                installingVersion: installingModpackVersion,
              }}
              navigation={{
                navItems,
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
                onOpenContextMenu: (profile, event) => {
                  event.preventDefault();
                  setHoverProfileId(null);
                  setProfileCtxMenu({
                    profileId: profile.id,
                    x: event.clientX,
                    y: event.clientY,
                  });
                },
                optionsCopySourceId,
                optionsCopyPulse,
                onOptionsCopyPick: (profile, event) => {
                  const point = optionsRipplePoint(event);
                  setOptionsCopySourceId(profile.id);
                  setOptionsCopyPulse({ profileId: profile.id, kind: "pick", ...point });
                },
                onOptionsCopyDrop: (profile, event) => {
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
                onActivate: (profile) => {
                  setActiveProfileId(profile.id);
                  if (savedAuth) handleLaunchGame(profile);
                  else handleWebviewLogin();
                },
                onDelete: handleDeleteProfile,
                totalCount: profiles.length,
              }}
              creation={{
                candidateVersions,
                creating,
                setCreating,
                createInputRef,
                labelForLoader: loaderDispLabel,
                handleCreateProfile: async (versionId, loader, name) => {
                  setSearchValue("");
                  return handleCreateProfile(versionId, loader, name);
                },
                launchAfterCreate: settings.game.launchAfterCreate !== false,
                loadersForCreate,
                hoverVersionKey,
                onHoverVersionKeyChange: setHoverVersionKey,
                intentEmpty: intent.empty,
                searchHasNoMatch:
                  !intent.empty &&
                  matchedProfiles.length === 0 &&
                  candidateVersions.length === 0,
                gameError: isGameBusy ? undefined : gameError,
                onClearGameError: () => setGameError(undefined),
              }}
              footer={{
                appVersion,
                accountsOpen: showAccounts,
                onVersionClick: () => {
                  advTapCountRef.current += 1;
                  if (advTapTimerRef.current) clearTimeout(advTapTimerRef.current);
                  if (advTapCountRef.current >= 7) {
                    advTapCountRef.current = 0;
                    if (!settings.advanced?.enabled) setShowAdvConfirm(true);
                  } else {
                    advTapTimerRef.current = setTimeout(() => {
                      advTapCountRef.current = 0;
                    }, 2000);
                  }
                },
                onToggleAccounts: () => setShowAccounts((open) => !open),
              }}
            />
          )}        </div>

        <LoginModal
          isOpen={loginModalOpen}
          state={loginState}
          errorMessage={errorMessage}
          onRetry={handleWebviewLogin}
          onClose={() => {
            setLoginModalOpen(false);
            setLoginState("idle");

            setErrorMessage(undefined);
          }}
        />

        {/* ── ログアウト確認ダイアログ ────────────────────────────────── */}
        {logoutConfirm && (
          <LogoutConfirmDialog
            username={logoutTarget?.username ?? savedAuth?.username}
            onCancel={() => {
              setLogoutConfirm(false);
              setLogoutTarget(null);
            }}
            onConfirm={() => {
              if (logoutTarget) handleLogoutAccount(logoutTarget);
              else handleLogout();
              setLogoutConfirm(false);
              setLogoutTarget(null);
            }}
          />
        )}

        {versionDialogModpack && (
          <ModpackVersionDialog
            modpack={versionDialogModpack}
            versions={modpackVersionsCache[versionDialogModpack.project_id] ?? []}
            isLoading={loadingVersionsFor === versionDialogModpack.project_id}
            installing={installingModpackVersion}
            focusedIndex={modpackVersionIdx}
            hoveredIndex={hoverModpackVersionIdx}
            onHoveredIndexChange={setHoverModpackVersionIdx}
            onClose={closeModpackVersionDialog}
            onInstall={async (version) => {
              if (installingModpackVersion) return;
              const modpack = versionDialogModpack;
              setInstallingModpackVersion({
                projectId: modpack.project_id,
                versionId: version.id,
                title: modpack.title,
              });
              setVersionDialogModpack(null);
              setHoverModpackVersionIdx(null);
              setSearchMode("profile");
              setSearchValue("");
              setTimeout(() => {
                inputRef.current?.focus();
                inputRef.current?.select();
              }, 50);
              try {
                await invoke("install_modpack_as_profile", {
                  projectId: modpack.project_id,
                  mcVersion: version.game_versions[0] ?? "1.20.1",
                  versionId: version.id,
                });
                refreshProfiles().catch(console.error);
              } catch (err) {
                console.error(err);
              } finally {
                setInstallingModpackVersion(null);
              }
            }}
          />
        )}      </>
    </ErrorBoundary>
  );
}
