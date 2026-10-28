import { untrack } from 'svelte';
import { type Setter, type Ref } from '../../lib/state';
import type { ActiveView, LoaderType, ModSearchResult, ModpackVersionInfo, Profile, StoredAuth, VersionManifest, } from "../../types";
import type { CreatingProfileDraft } from "../profileDraft";
import { hasDocumentSelection, isEditableTarget } from "../../utils/dom";
import { resolveMainEnterTarget } from "../../utils/navigation";
import { createTimeouts } from '../timeouts';
type SearchMode = "profile" | "modpack";
type Args = {
    activeView: ActiveView;
    setActiveView: Setter<ActiveView>;
    navDirRef: Ref<"forward" | "back" | "none">;
    showAccounts: boolean;
    deleteConfirmId: string | null;
    logoutConfirm: boolean;
    showAdvConfirm: boolean;
    configProfileId: string | null;
    loginModalOpenRef: Ref<boolean>;
    inputRef: Ref<HTMLInputElement | null>;
    createInputRef: Ref<HTMLInputElement | null>;
    isMacOS: boolean;
    searchMode: SearchMode;
    onToggleVersionChannel: () => void;
    onCompleteSearchCommand: () => boolean;
    onExitSearchMode: () => void;
    searchValue: string;
    creating: CreatingProfileDraft | null;
    setCreating: Setter<CreatingProfileDraft | null>;
    navIndexRef: Ref<number>;
    navItemsRef: Ref<string[]>;
    navElemsRef: Ref<Map<string, HTMLElement>>;
    setNavIndex: Setter<number>;
    setHoverProfileId: Setter<string | null>;
    setHoverModpackIdx: Setter<number | null>;
    setHoverVersionKey: Setter<string | null>;
    hoverProfileId: string | null;
    hoverVersionKey: string | null;
    profiles: Profile[];
    activeProfileId: string | null;
    setActiveProfileId: Setter<string | null>;
    savedAuth: StoredAuth | null;
    manifest: VersionManifest | null;
    modpackResults: ModSearchResult[];
    openModpackVersionDialog: (modpack: ModSearchResult) => void;
    versionDialogModpack: ModSearchResult | null;
    modpackVersionsCache: Record<string, ModpackVersionInfo[]>;
    modpackVersionIdx: number;
    setModpackVersionIdx: Setter<number>;
    installingModpackVersion: {
        projectId: string;
        versionId: string;
        title?: string;
    } | null;
    setConfigProfileId: Setter<string | null>;
    setModsProfileId: Setter<string | null>;
    handleDeleteProfile: (profileId: string) => void;
    isProfileBusy: (profileId: string) => boolean;
    handleLaunchGame: (profile: Profile) => void | Promise<void>;
    handleWebviewLogin: () => void | Promise<void>;
};
export function useMainCommandKeydown(params: Args) {
    const timers = createTimeouts();
    $effect(() => {
        void params.activeView;
        void params.activeProfileId;
        void params.configProfileId;
        void params.createInputRef;
        void params.creating;
        void params.deleteConfirmId;
        void params.handleDeleteProfile;
        void params.handleLaunchGame;
        void params.handleWebviewLogin;
        void params.hoverProfileId;
        void params.hoverVersionKey;
        void params.inputRef;
        void params.installingModpackVersion;
        void params.isMacOS;
        void params.isProfileBusy;
        void params.loginModalOpenRef;
        void params.logoutConfirm;
        void params.manifest;
        void params.modpackResults;
        void params.modpackVersionIdx;
        void params.modpackVersionsCache;
        void params.navDirRef;
        void params.navElemsRef;
        void params.navIndexRef;
        void params.navItemsRef;
        void params.openModpackVersionDialog;
        void params.profiles;
        void params.savedAuth;
        void params.searchMode;
        void params.searchValue;
        void params.setActiveProfileId;
        void params.setActiveView;
        void params.setConfigProfileId;
        void params.setCreating;
        void params.setHoverModpackIdx;
        void params.setHoverProfileId;
        void params.setHoverVersionKey;
        void params.setModpackVersionIdx;
        void params.setModsProfileId;
        void params.setNavIndex;
        void params.onToggleVersionChannel;
        void params.onCompleteSearchCommand;
        void params.onExitSearchMode;
        void params.showAccounts;
        void params.showAdvConfirm;
        void params.versionDialogModpack;
        return untrack(() => {
            const handleKeyDown = (event: KeyboardEvent) => {
                if ((event.ctrlKey || event.metaKey) && event.key === ",") {
                    event.preventDefault();
                    params.navDirRef.current = "forward";
                    params.setActiveView("settings");
                    return;
                }
                if ((event.ctrlKey || event.metaKey) && event.key === "k") {
                    event.preventDefault();
                    if (params.activeView === "main" && !params.showAccounts) {
                        const current = params.navIndexRef.current;
                        const key = params.navItemsRef.current[current];
                        if (key?.startsWith("p:")) {
                            params.setConfigProfileId(key.slice(2));
                        }
                        else if (params.activeProfileId) {
                            params.setConfigProfileId(params.activeProfileId);
                        }
                    }
                    return;
                }
                if (params.activeView !== "main" || params.showAccounts || params.deleteConfirmId || params.logoutConfirm || params.showAdvConfirm || params.configProfileId ||
                    params.loginModalOpenRef.current) {
                    return;
                }
                if (event.key === "Backspace" && event.target === params.inputRef.current) {
                    if (params.searchMode === "modpack" && params.searchValue === "" &&
                        !params.creating && !params.versionDialogModpack && !event.isComposing && !event.repeat &&
                        !event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey) {
                        event.preventDefault();
                        params.onExitSearchMode();
                    }
                    return;
                }
                if (event.key === " " && event.target === params.inputRef.current) {
                    if (!event.isComposing && !event.repeat &&
                        !event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey &&
                        params.searchMode === "profile" && !params.creating && !params.versionDialogModpack &&
                        params.inputRef.current?.selectionStart === params.searchValue.length &&
                        params.inputRef.current?.selectionEnd === params.searchValue.length &&
                        params.onCompleteSearchCommand()) event.preventDefault();
                    return;
                }
                if (event.key === "Tab") {
                    if (event.shiftKey || event.ctrlKey || event.metaKey || event.altKey ||
                        params.searchMode !== "profile" || params.creating || params.versionDialogModpack) return;
                    if (event.target instanceof HTMLElement && event.target !== params.inputRef.current &&
                        event.target.closest("input, textarea, select, button, a, [tabindex], [contenteditable], [role='menu'], [role='dialog']")) return;
                    event.preventDefault();
                    if (!event.repeat) params.onToggleVersionChannel();
                    return;
                }
                if ((event.ctrlKey || event.metaKey) && event.key === "a") {
                    if (isEditableTarget(event.target))
                        return;
                    if (hasDocumentSelection())
                        return;
                    if (event.target instanceof HTMLElement &&
                        (event.target.closest(".log-body") ||
                            event.target.closest("[data-selectable]"))) {
                        return;
                    }
                    event.preventDefault();
                    params.inputRef.current?.focus();
                    params.inputRef.current?.select();
                    return;
                }
                if ((event.ctrlKey || (params.isMacOS && event.metaKey)) && !event.altKey) {
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
                if (event.altKey &&
                    event.key === "ArrowRight" &&
                    !event.ctrlKey &&
                    !event.metaKey &&
                    !params.configProfileId) {
                    event.preventDefault();
                    const navItem = params.navItemsRef.current[params.navIndexRef.current];
                    if (navItem?.startsWith("p:")) {
                        const profileId = navItem.slice(2);
                        const target = params.profiles.find((profile) => profile.id === profileId && profile.loader !== "vanilla");
                        if (target) {
                            params.navDirRef.current = "forward";
                            params.setModsProfileId(target.id);
                            params.setActiveView("mods");
                        }
                    }
                    return;
                }
                if (event.altKey && event.key === "ArrowLeft" && !event.ctrlKey && !event.metaKey) {
                    event.preventDefault();
                    const current = params.navIndexRef.current;
                    if (params.searchMode === "profile") {
                        const key = params.navItemsRef.current[current];
                        if (key?.startsWith("p:")) {
                            params.handleDeleteProfile(key.slice(2));
                        }
                    }
                    return;
                }
                if (event.metaKey || event.ctrlKey)
                    return;
                if (event.altKey) {
                    event.preventDefault();
                    return;
                }
                if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                    event.preventDefault();
                    if (params.versionDialogModpack) {
                        const versions = params.modpackVersionsCache[params.versionDialogModpack.project_id] ?? [];
                        if (versions.length > 0) {
                            params.setModpackVersionIdx((index) => event.key === "ArrowDown"
                                ? Math.min(index + 1, versions.length - 1)
                                : Math.max(index - 1, 0));
                        }
                        return;
                    }
                    const items = params.navItemsRef.current;
                    if (params.creating || items.length === 0) {
                        params.inputRef.current?.focus();
                        return;
                    }
                    params.setHoverProfileId(null);
                    params.setHoverModpackIdx(null);
                    params.setHoverVersionKey(null);
                    const len = items.length;
                    const current = params.navIndexRef.current;
                    let next = current;
                    if (event.key === "ArrowDown") {
                        if (current >= len - 1)
                            next = event.repeat ? len - 1 : 0;
                        else
                            next = current < 0 ? 0 : current + 1;
                    }
                    else if (current <= 0) {
                        next = current < 0 ? len - 1 : event.repeat ? 0 : len - 1;
                    }
                    else {
                        next = current - 1;
                    }
                    params.navIndexRef.current = next;
                    params.setNavIndex(next);
                    return;
                }
                if (event.key === "Enter" && params.versionDialogModpack) {
                    event.preventDefault();
                    const versions = params.modpackVersionsCache[params.versionDialogModpack.project_id] ?? [];
                    if (versions[params.modpackVersionIdx] && params.installingModpackVersion === null) {
                        document
                            .querySelector<HTMLElement>(`[data-modpack-version-idx="${params.modpackVersionIdx}"]`)
                            ?.click();
                    }
                    return;
                }
                if (event.key === "Enter" && params.searchMode === "modpack" && !params.creating) {
                    event.preventDefault();
                    const current = params.navIndexRef.current;
                    if (current >= 0 && current < params.modpackResults.length) {
                        params.openModpackVersionDialog(params.modpackResults[current]);
                    }
                    return;
                }
                if (event.key === "Enter" &&
                    params.searchMode === "profile" &&
                    !params.creating &&
                    !params.versionDialogModpack) {
                    event.preventDefault();
                    const target = resolveMainEnterTarget({
                        navItems: params.navItemsRef.current,
                        navIndex: params.navIndexRef.current,
                        hoverProfileId: params.hoverProfileId,
                        hoverVersionKey: params.hoverVersionKey,
                    });
                    if (target?.kind === "profile") {
                        const profile = params.profiles.find((item) => item.id === target.profileId);
                        if (profile && !params.isProfileBusy(profile.id)) {
                            params.setActiveProfileId(profile.id);
                            if (params.savedAuth)
                                params.handleLaunchGame(profile);
                            else
                                params.handleWebviewLogin();
                        }
                    }
                    else if (target?.kind === "create") {
                        if (params.profiles.length > 0 && params.searchValue.trim().length === 0) {
                            return;
                        }
                        const versionType = params.manifest?.versions.find((version) => version.id === target.versionId)?.type;
                        params.setCreating({
                            versionId: target.versionId,
                            versionType,
                            loader: target.loader as LoaderType,
                            inputName: "",
                        });
                        timers.set(() => params.createInputRef.current?.focus(), 30);
                    }
                    return;
                }
                if (event.target instanceof HTMLInputElement ||
                    event.target instanceof HTMLTextAreaElement) {
                    return;
                }
                if (event.key.length === 1 || event.key === "Backspace") {
                    if (document.activeElement !== params.inputRef.current) {
                        params.inputRef.current?.focus();
                    }
                }
            };
            const clickNavItemByIndex = (index: number) => {
                const items = params.navItemsRef.current;
                if (index >= items.length)
                    return;
                params.setNavIndex(index);
                params.navIndexRef.current = index;
                timers.set(() => {
                    const key = items[index];
                    params.navElemsRef.current.get(key)?.click();
                }, 50);
            };
            window.addEventListener("keydown", handleKeyDown);
            return () => window.removeEventListener("keydown", handleKeyDown);
        });
    });
}
