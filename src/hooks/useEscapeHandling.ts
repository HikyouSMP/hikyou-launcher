import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ActiveView, LoginState, ModSearchResult } from "../types";
import type { CreatingProfileDraft } from "../components/CreateCandidateList";

type Args = {
  creating: CreatingProfileDraft | null;
  setCreating: React.Dispatch<React.SetStateAction<CreatingProfileDraft | null>>;
  configProfileId: string | null;
  setConfigProfileId: React.Dispatch<React.SetStateAction<string | null>>;
  modsProfileId: string | null;
  setModsProfileId: React.Dispatch<React.SetStateAction<string | null>>;
  activeView: ActiveView;
  setActiveView: React.Dispatch<React.SetStateAction<ActiveView>>;
  showAccounts: boolean;
  setShowAccounts: React.Dispatch<React.SetStateAction<boolean>>;
  searchMode: "profile" | "modpack";
  setSearchMode: React.Dispatch<React.SetStateAction<"profile" | "modpack">>;
  searchValue: string;
  setSearchValue: React.Dispatch<React.SetStateAction<string>>;
  versionDialogOpen: boolean;
  closeVersionDialog: () => void;
  logoutConfirm: boolean;
  setLogoutConfirm: React.Dispatch<React.SetStateAction<boolean>>;
  deleteConfirmId: string | null;
  setDeleteConfirmId: React.Dispatch<React.SetStateAction<string | null>>;
  profileCtxMenuOpen: boolean;
  closeProfileCtxMenu: () => void;
  loginModalOpenRef: React.MutableRefObject<boolean>;
  setLoginModalOpen: React.Dispatch<React.SetStateAction<boolean>>;
  setLoginState: React.Dispatch<React.SetStateAction<LoginState>>;
  setErrorMessage: React.Dispatch<React.SetStateAction<string | undefined>>;
  setModpackResults: React.Dispatch<React.SetStateAction<ModSearchResult[]>>;
  setNavIndex: React.Dispatch<React.SetStateAction<number>>;
  navIndexRef: React.MutableRefObject<number>;
  navDirRef: React.MutableRefObject<"forward" | "back" | "none">;
  inputRef: React.RefObject<HTMLInputElement | null>;
  isMainView: boolean;
  onDeleteConfirmEnter: () => Promise<void>;
  isDeleteTargetBusy: boolean;
};

export function useEscapeHandling({
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
  versionDialogOpen,
  closeVersionDialog,
  logoutConfirm,
  setLogoutConfirm,
  deleteConfirmId,
  setDeleteConfirmId,
  profileCtxMenuOpen,
  closeProfileCtxMenu,
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
  onDeleteConfirmEnter,
  isDeleteTargetBusy,
}: Args) {
  useEffect(() => {
    const handler = async (event: KeyboardEvent) => {
      if (deleteConfirmId && event.key === "Enter") {
        event.preventDefault();
        if (!isDeleteTargetBusy) await onDeleteConfirmEnter();
        return;
      }
      if (event.key !== "Escape") return;
      event.preventDefault();
      if (profileCtxMenuOpen) {
        closeProfileCtxMenu();
        return;
      }
      if (logoutConfirm) {
        setLogoutConfirm(false);
        return;
      }
      if (activeView === "rec-mods") {
        navDirRef.current = "back";
        setActiveView("settings");
        return;
      }
      if (versionDialogOpen) {
        closeVersionDialog();
        return;
      }
      if (deleteConfirmId) {
        setDeleteConfirmId(null);
        return;
      }
      if (configProfileId) {
        setConfigProfileId(null);
        return;
      }
      if (creating) {
        setCreating(null);
        return;
      }
      if (searchValue.trim()) {
        setSearchValue("");
        return;
      }
      if (modsProfileId) {
        navDirRef.current = "forward";
        setModsProfileId(null);
        setActiveView("main");
        setTimeout(() => {
          inputRef.current?.focus();
          inputRef.current?.select();
        }, 50);
        return;
      }
      if (!isMainView) {
        navDirRef.current = "forward";
        setActiveView("main");
        setTimeout(() => {
          inputRef.current?.focus();
          inputRef.current?.select();
        }, 50);
        return;
      }
      if (searchMode === "modpack") {
        setSearchMode("profile");
        setModpackResults([]);
        setNavIndex(-1);
        navIndexRef.current = -1;
        return;
      }
      if (showAccounts) {
        setShowAccounts(false);
        setLogoutConfirm(false);
        return;
      }
      if (loginModalOpenRef.current) {
        setLoginModalOpen(false);
        setLoginState("idle");
        setErrorMessage(undefined);
        return;
      }
      await invoke("hide_main_window", { reason: "escape" }).catch(
        console.error,
      );
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [
    activeView,
    closeProfileCtxMenu,
    closeVersionDialog,
    configProfileId,
    creating,
    deleteConfirmId,
    inputRef,
    isDeleteTargetBusy,
    isMainView,
    loginModalOpenRef,
    logoutConfirm,
    modsProfileId,
    navDirRef,
    navIndexRef,
    onDeleteConfirmEnter,
    profileCtxMenuOpen,
    searchMode,
    searchValue,
    setActiveView,
    setConfigProfileId,
    setCreating,
    setDeleteConfirmId,
    setErrorMessage,
    setLoginModalOpen,
    setLoginState,
    setLogoutConfirm,
    setModpackResults,
    setModsProfileId,
    setNavIndex,
    setSearchMode,
    setSearchValue,
    setShowAccounts,
    showAccounts,
    versionDialogOpen,
  ]);
}
