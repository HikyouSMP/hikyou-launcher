import { useEffect } from "react";

import type {
  ActiveView,
  ModSearchResult,
  ModpackVersionInfo,
  Profile,
  StoredAuth,
  VersionManifest,
} from "../types";
import type { CreatingProfileDraft } from "../components/CreateCandidateList";
import { useCtrlLaunchBadges } from "./navigation/useCtrlLaunchBadges";
import { useMainInputFocusRedirect } from "./navigation/useMainInputFocusRedirect";
import { useModpackSearch } from "./navigation/useModpackSearch";
import { useMainCommandKeydown } from "./navigation/useMainCommandKeydown";
import { useNavigationScroll } from "./navigation/useNavigationScroll";

type SearchMode = "profile" | "modpack";

interface UseCommandNavigationParams {
  activeView: ActiveView;
  setActiveView: React.Dispatch<React.SetStateAction<ActiveView>>;
  navDirRef: React.MutableRefObject<"forward" | "back" | "none">;

  showAccounts: boolean;
  deleteConfirmId: string | null;
  logoutConfirm: boolean;
  showAdvConfirm: boolean;
  configProfileId: string | null;
  loginModalOpenRef: React.MutableRefObject<boolean>;

  inputRef: React.RefObject<HTMLInputElement | null>;
  createInputRef: React.RefObject<HTMLInputElement | null>;
  isMacOS: boolean;

  searchMode: SearchMode;
  setSearchMode: React.Dispatch<React.SetStateAction<SearchMode>>;
  searchValue: string;
  creating: CreatingProfileDraft | null;
  setCreating: React.Dispatch<React.SetStateAction<CreatingProfileDraft | null>>;

  navIndexRef: React.MutableRefObject<number>;
  navItemsRef: React.MutableRefObject<string[]>;
  navElemsRef: React.MutableRefObject<Map<string, HTMLElement>>;
  navIndex: number;
  setNavIndex: React.Dispatch<React.SetStateAction<number>>;

  ctrlHeld: boolean;
  setCtrlHeld: React.Dispatch<React.SetStateAction<boolean>>;
  setHoverProfileId: React.Dispatch<React.SetStateAction<string | null>>;
  setHoverModpackIdx: React.Dispatch<React.SetStateAction<number | null>>;
  setHoverVersionKey: React.Dispatch<React.SetStateAction<string | null>>;
  hoverProfileId: string | null;
  hoverVersionKey: string | null;

  profiles: Profile[];
  activeProfileId: string | null;
  setActiveProfileId: React.Dispatch<React.SetStateAction<string | null>>;
  savedAuth: StoredAuth | null;
  manifest: VersionManifest | null;

  modpackResults: ModSearchResult[];
  setModpackResults: React.Dispatch<React.SetStateAction<ModSearchResult[]>>;
  setModpackSearching: React.Dispatch<React.SetStateAction<boolean>>;
  openModpackVersionDialog: (modpack: ModSearchResult) => void;

  versionDialogModpack: ModSearchResult | null;
  modpackVersionsCache: Record<string, ModpackVersionInfo[]>;
  modpackVersionIdx: number;
  setModpackVersionIdx: React.Dispatch<React.SetStateAction<number>>;
  installingModpackVersion: { projectId: string; versionId: string; title?: string } | null;

  setConfigProfileId: React.Dispatch<React.SetStateAction<string | null>>;
  setModsProfileId: React.Dispatch<React.SetStateAction<string | null>>;
  handleDeleteProfile: (profileId: string) => void;
  isProfileBusy: (profileId: string) => boolean;
  handleLaunchGame: (profile: Profile) => void | Promise<void>;
  handleWebviewLogin: () => void | Promise<void>;
}

export function useCommandNavigation({
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
}: UseCommandNavigationParams) {
  useNavigationScroll({
    navIndex,
    navItemsRef,
    navElemsRef,
    versionDialogOpen: Boolean(versionDialogModpack),
    modpackVersionIdx,
  });

  useEffect(() => {
    navIndexRef.current = navIndex;
  }, [navIndex, navIndexRef]);

  useCtrlLaunchBadges({ isMacOS, ctrlHeld, setCtrlHeld, navItemsRef });

  useEffect(() => {
    if (activeView !== "main") {
      setSearchMode("profile");
      setModpackResults([]);
    }
  }, [activeView, setModpackResults, setSearchMode]);

  useMainInputFocusRedirect({ active: activeView === "main", inputRef });

  useModpackSearch({
    active: searchMode === "modpack",
    query: searchValue,
    setModpackResults,
    setModpackSearching,
  });

  useEffect(() => {
    // A search changes the command set. Keep a visible keyboard target whenever
    // one exists instead of clearing selection after the result list rendered.
    const nextIndex = navItemsRef.current.length > 0 ? 0 : -1;
    setNavIndex(nextIndex);
    navIndexRef.current = nextIndex;
  }, [creating, navIndexRef, searchMode, searchValue, setNavIndex]);

  useMainCommandKeydown({
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
    setNavIndex,
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

}
