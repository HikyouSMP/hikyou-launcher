import { untrack } from 'svelte';
import { type Setter, type Ref } from '../lib/state';
import { invoke } from "@tauri-apps/api/core";
import { createTimeouts } from './timeouts';
import type { ActiveView, LoginState } from "../types";
import type { CreatingProfileDraft } from "./profileDraft";
type Args = {
    creating: CreatingProfileDraft | null;
    setCreating: Setter<CreatingProfileDraft | null>;
    configProfileId: string | null;
    setConfigProfileId: Setter<string | null>;
    modsProfileId: string | null;
    setModsProfileId: Setter<string | null>;
    modsReturnView: "main" | "library";
    activeView: ActiveView;
    setActiveView: Setter<ActiveView>;
    showAccounts: boolean;
    setShowAccounts: Setter<boolean>;
    searchMode: "profile" | "modpack";
    onExitSearchMode: () => void;
    searchValue: string;
    setSearchValue: Setter<string>;
    versionDialogOpen: boolean;
    closeVersionDialog: () => void;
    logoutConfirm: boolean;
    setLogoutConfirm: Setter<boolean>;
    deleteConfirmId: string | null;
    setDeleteConfirmId: Setter<string | null>;
    profileCtxMenuOpen: boolean;
    closeProfileCtxMenu: () => void;
    loginModalOpenRef: Ref<boolean>;
    setLoginModalOpen: Setter<boolean>;
    setLoginState: Setter<LoginState>;
    setErrorMessage: Setter<string | undefined>;
    navDirRef: Ref<"forward" | "back" | "none">;
    inputRef: Ref<HTMLInputElement | null>;
    isMainView: boolean;
    onDeleteConfirmEnter: () => Promise<void>;
    isDeleteTargetBusy: boolean;
};
export function useEscapeHandling(params: Args) {
    const timers = createTimeouts();
    $effect(() => {
        void params.activeView;
        void params.closeProfileCtxMenu;
        void params.closeVersionDialog;
        void params.configProfileId;
        void params.creating;
        void params.deleteConfirmId;
        void params.inputRef;
        void params.isDeleteTargetBusy;
        void params.isMainView;
        void params.loginModalOpenRef;
        void params.logoutConfirm;
        void params.modsProfileId;
        void params.modsReturnView;
        void params.navDirRef;
        void params.onDeleteConfirmEnter;
        void params.profileCtxMenuOpen;
        void params.searchMode;
        void params.searchValue;
        void params.setActiveView;
        void params.setConfigProfileId;
        void params.setCreating;
        void params.setDeleteConfirmId;
        void params.setErrorMessage;
        void params.setLoginModalOpen;
        void params.setLoginState;
        void params.setLogoutConfirm;
        void params.setModsProfileId;
        void params.onExitSearchMode;
        void params.setSearchValue;
        void params.setShowAccounts;
        void params.showAccounts;
        void params.versionDialogOpen;
        return untrack(() => {
            const handler = async (event: KeyboardEvent) => {
                if (params.deleteConfirmId && event.key === "Enter") {
                    event.preventDefault();
                    if (!params.isDeleteTargetBusy)
                        await params.onDeleteConfirmEnter();
                    return;
                }
                if (event.key !== "Escape")
                    return;
                event.preventDefault();
                if (params.profileCtxMenuOpen) {
                    params.closeProfileCtxMenu();
                    return;
                }
                if (params.logoutConfirm) {
                    params.setLogoutConfirm(false);
                    return;
                }
                if (params.activeView === "rec-mods") {
                    params.navDirRef.current = "back";
                    params.setActiveView("settings");
                    return;
                }
                if (params.versionDialogOpen) {
                    params.closeVersionDialog();
                    return;
                }
                if (params.deleteConfirmId) {
                    params.setDeleteConfirmId(null);
                    return;
                }
                if (params.configProfileId) {
                    params.setConfigProfileId(null);
                    return;
                }
                if (params.creating) {
                    params.setCreating(null);
                    return;
                }
                if (params.searchValue.trim()) {
                    params.setSearchValue("");
                    return;
                }
                if (params.modsProfileId) {
                    params.navDirRef.current = "forward";
                    params.setModsProfileId(null);
                    params.setActiveView(params.modsReturnView);
                    timers.set(() => {
                        params.inputRef.current?.focus();
                        params.inputRef.current?.select();
                    }, 50);
                    return;
                }
                if (!params.isMainView) {
                    params.navDirRef.current = "forward";
                    params.setActiveView("main");
                    timers.set(() => {
                        params.inputRef.current?.focus();
                        params.inputRef.current?.select();
                    }, 50);
                    return;
                }
                if (params.searchMode === "modpack") {
                    params.onExitSearchMode();
                    return;
                }
                if (params.showAccounts) {
                    params.setShowAccounts(false);
                    params.setLogoutConfirm(false);
                    return;
                }
                if (params.loginModalOpenRef.current) {
                    await invoke("cancel_auth_login").catch(console.error);
                    params.setLoginModalOpen(false);
                    params.setLoginState("idle");
                    params.setErrorMessage(undefined);
                    return;
                }
                await invoke("hide_main_window", { reason: "escape" }).catch(console.error);
            };
            window.addEventListener("keydown", handler);
            return () => window.removeEventListener("keydown", handler);
        });
    });
}
