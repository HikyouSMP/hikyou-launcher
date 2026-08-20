import { useEffect } from "react";

import type {
  ActiveView,
  LoaderType,
  ModSearchResult,
  ModpackVersionInfo,
  Profile,
  StoredAuth,
  VersionManifest,
} from "../../types";
import type { CreatingProfileDraft } from "../../components/CreateCandidateList";
import { hasDocumentSelection, isEditableTarget } from "../../utils/dom";
import { resolveMainEnterTarget } from "../../utils/navigation";
import { searchModpacksNow } from "./navigationActions";

type SearchMode = "profile" | "modpack";

type Args = {
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
  setNavIndex: React.Dispatch<React.SetStateAction<number>>;
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
};

export function useMainCommandKeydown({
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
}: Args) {
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key === ",") {
        event.preventDefault();
        navDirRef.current = "forward";
        setActiveView("settings");
        return;
      }

      if ((event.ctrlKey || event.metaKey) && event.key === "k") {
        event.preventDefault();
        if (activeView === "main" && !showAccounts) {
          const current = navIndexRef.current;
          const key = navItemsRef.current[current];
          if (key?.startsWith("p:")) {
            setConfigProfileId(key.slice(2));
          } else if (activeProfileId) {
            setConfigProfileId(activeProfileId);
          }
        }
        return;
      }

      if (
        activeView !== "main" ||
        showAccounts ||
        deleteConfirmId ||
        logoutConfirm ||
        showAdvConfirm ||
        configProfileId ||
        loginModalOpenRef.current
      ) {
        return;
      }

      if (event.key === "Tab") {
        event.preventDefault();
        if (searchMode === "profile") {
          setSearchMode("modpack");
          setModpackResults([]);
          searchModpacksNow(
            searchValue,
            setModpackResults,
            setModpackSearching,
          );
        } else {
          setSearchMode("profile");
          setModpackResults([]);
          setNavIndex(-1);
          navIndexRef.current = -1;
        }
        return;
      }

      if ((event.ctrlKey || event.metaKey) && event.key === "a") {
        if (isEditableTarget(event.target)) return;
        if (hasDocumentSelection()) return;
        if (
          event.target instanceof HTMLElement &&
          (event.target.closest(".log-body") ||
            event.target.closest("[data-selectable]"))
        ) {
          return;
        }
        event.preventDefault();
        inputRef.current?.focus();
        inputRef.current?.select();
        return;
      }

      if ((event.ctrlKey || (isMacOS && event.metaKey)) && !event.altKey) {
        if (event.key >= "1" && event.key <= "9") {
          event.preventDefault();
          clickNavItemByIndex(Number.parseInt(event.key, 10) - 1);
          return;
        }
        if (event.key === "0") {
          event.preventDefault();
          clickNavItemByIndex(9);
          return;
        }
      }

      if (
        event.altKey &&
        event.key === "ArrowRight" &&
        !event.ctrlKey &&
        !event.metaKey &&
        !configProfileId
      ) {
        event.preventDefault();
        const navItem = navItemsRef.current[navIndexRef.current];
        if (navItem?.startsWith("p:")) {
          const profileId = navItem.slice(2);
          const target = profiles.find(
            (profile) => profile.id === profileId && profile.loader !== "vanilla",
          );
          if (target) {
            navDirRef.current = "forward";
            setModsProfileId(target.id);
            setActiveView("mods");
          }
        }
        return;
      }

      if (event.altKey && event.key === "ArrowLeft" && !event.ctrlKey && !event.metaKey) {
        event.preventDefault();
        const current = navIndexRef.current;
        if (searchMode === "profile") {
          const key = navItemsRef.current[current];
          if (key?.startsWith("p:")) {
            handleDeleteProfile(key.slice(2));
          }
        }
        return;
      }

      if (event.metaKey || event.ctrlKey) return;
      if (event.altKey) {
        event.preventDefault();
        return;
      }

      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        if (versionDialogModpack) {
          const versions =
            modpackVersionsCache[versionDialogModpack.project_id] ?? [];
          if (versions.length > 0) {
            setModpackVersionIdx((index) =>
              event.key === "ArrowDown"
                ? Math.min(index + 1, versions.length - 1)
                : Math.max(index - 1, 0),
            );
          }
          return;
        }

        const items = navItemsRef.current;
        if (creating || items.length === 0) {
          inputRef.current?.focus();
          return;
        }
        setHoverProfileId(null);
        setHoverModpackIdx(null);
        setHoverVersionKey(null);

        const len = items.length;
        const current = navIndexRef.current;
        let next = current;
        if (event.key === "ArrowDown") {
          if (current >= len - 1) next = event.repeat ? len - 1 : 0;
          else next = current < 0 ? 0 : current + 1;
        } else if (current <= 0) {
          next = current < 0 ? len - 1 : event.repeat ? 0 : len - 1;
        } else {
          next = current - 1;
        }

        navIndexRef.current = next;
        setNavIndex(next);
        return;
      }

      if (event.key === "Enter" && versionDialogModpack) {
        event.preventDefault();
        const versions =
          modpackVersionsCache[versionDialogModpack.project_id] ?? [];
        if (versions[modpackVersionIdx] && installingModpackVersion === null) {
          document
            .querySelector<HTMLElement>(
              `[data-modpack-version-idx="${modpackVersionIdx}"]`,
            )
            ?.click();
        }
        return;
      }

      if (event.key === "Enter" && searchMode === "modpack" && !creating) {
        event.preventDefault();
        const current = navIndexRef.current;
        if (current >= 0 && current < modpackResults.length) {
          openModpackVersionDialog(modpackResults[current]);
        }
        return;
      }

      if (
        event.key === "Enter" &&
        searchMode === "profile" &&
        !creating &&
        !versionDialogModpack
      ) {
        event.preventDefault();
        const target = resolveMainEnterTarget({
          navItems: navItemsRef.current,
          navIndex: navIndexRef.current,
          hoverProfileId,
          hoverVersionKey,
        });
        if (target?.kind === "profile") {
          const profile = profiles.find((item) => item.id === target.profileId);
          if (profile && !isProfileBusy(profile.id)) {
            setActiveProfileId(profile.id);
            if (savedAuth) handleLaunchGame(profile);
            else handleWebviewLogin();
          }
        } else if (target?.kind === "create") {
          if (profiles.length > 0 && searchValue.trim().length === 0) {
            return;
          }
          const versionType = manifest?.versions.find(
            (version) => version.id === target.versionId,
          )?.type;
          setCreating({
            versionId: target.versionId,
            versionType,
            loader: target.loader as LoaderType,
            inputName: "",
          });
          setTimeout(() => createInputRef.current?.focus(), 30);
        }
        return;
      }

      if (
        event.target instanceof HTMLInputElement ||
        event.target instanceof HTMLTextAreaElement
      ) {
        return;
      }
      if (event.key.length === 1 || event.key === "Backspace") {
        if (document.activeElement !== inputRef.current) {
          inputRef.current?.focus();
        }
        setNavIndex(-1);
      }
    };

    const clickNavItemByIndex = (index: number) => {
      const items = navItemsRef.current;
      if (index >= items.length) return;
      setNavIndex(index);
      navIndexRef.current = index;
      setTimeout(() => {
        const key = items[index];
        navElemsRef.current.get(key)?.click();
      }, 50);
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [
    activeView,
    activeProfileId,
    configProfileId,
    createInputRef,
    creating,
    deleteConfirmId,
    handleDeleteProfile,
    handleLaunchGame,
    handleWebviewLogin,
    hoverProfileId,
    hoverVersionKey,
    inputRef,
    installingModpackVersion,
    isMacOS,
    isProfileBusy,
    loginModalOpenRef,
    logoutConfirm,
    manifest,
    modpackResults,
    modpackVersionIdx,
    modpackVersionsCache,
    navDirRef,
    navElemsRef,
    navIndexRef,
    navItemsRef,
    openModpackVersionDialog,
    profiles,
    savedAuth,
    searchMode,
    searchValue,
    setActiveProfileId,
    setActiveView,
    setConfigProfileId,
    setCreating,
    setHoverModpackIdx,
    setHoverProfileId,
    setHoverVersionKey,
    setModpackResults,
    setModpackSearching,
    setModpackVersionIdx,
    setModsProfileId,
    setNavIndex,
    setSearchMode,
    showAccounts,
    showAdvConfirm,
    versionDialogModpack,
  ]);
}
