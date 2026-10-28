import { untrack } from 'svelte';
import { type Setter, type Ref } from '../lib/state';
import type { ActiveView, ModSearchResult, ModpackVersionInfo, Profile, StoredAuth, VersionManifest, VersionChannel } from "../types";
import type { CreatingProfileDraft } from "./profileDraft";
import { useCtrlLaunchBadges } from "./navigation/useCtrlLaunchBadges.svelte";
import { useMainInputFocusRedirect } from "./navigation/useMainInputFocusRedirect.svelte";
import { useModpackSearch } from "./navigation/useModpackSearch.svelte";
import { useMainCommandKeydown } from "./navigation/useMainCommandKeydown.svelte";
import { useNavigationScroll } from "./navigation/useNavigationScroll.svelte";
type SearchMode = "profile" | "modpack";
interface UseCommandNavigationParams {
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
    setSearchMode: Setter<SearchMode>;
    onToggleVersionChannel: () => void;
    onCompleteSearchCommand: () => boolean;
    onExitSearchMode: () => void;
    versionChannel: VersionChannel;
    searchValue: string;
    creating: CreatingProfileDraft | null;
    setCreating: Setter<CreatingProfileDraft | null>;
    navIndexRef: Ref<number>;
    navItemsRef: Ref<string[]>;
    navElemsRef: Ref<Map<string, HTMLElement>>;
    navIndex: number;
    setNavIndex: Setter<number>;
    ctrlHeld: boolean;
    setCtrlHeld: Setter<boolean>;
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
    setModpackResults: Setter<ModSearchResult[]>;
    setModpackSearching: Setter<boolean>;
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
}
export function useCommandNavigation(params: UseCommandNavigationParams) {
    useNavigationScroll({
        get navIndex() {
            return params.navIndex;
        },
        get navItemsRef() {
            return params.navItemsRef;
        },
        get navElemsRef() {
            return params.navElemsRef;
        },
        get versionDialogOpen() {
            return Boolean(params.versionDialogModpack);
        },
        get modpackVersionIdx() {
            return params.modpackVersionIdx;
        }
    });
    $effect(() => {
        void params.navIndex;
        void params.navIndexRef;
        return untrack(() => {
            params.navIndexRef.current = params.navIndex;
        });
    });
    useCtrlLaunchBadges({
        get isMacOS() {
            return params.isMacOS;
        },
        get ctrlHeld() {
            return params.ctrlHeld;
        },
        get setCtrlHeld() {
            return params.setCtrlHeld;
        },
        get navItemsRef() {
            return params.navItemsRef;
        }
    });
    $effect(() => {
        void params.activeView;
        void params.setModpackResults;
        void params.setSearchMode;
        return untrack(() => {
            if (params.activeView !== "main") {
                params.setSearchMode("profile");
                params.setModpackResults([]);
            }
        });
    });
    useMainInputFocusRedirect({
        get active() {
            return params.activeView === "main";
        },
        get inputRef() {
            return params.inputRef;
        }
    });
    useModpackSearch({
        get active() {
            return params.searchMode === "modpack";
        },
        get query() {
            return params.searchValue;
        },
        get setModpackResults() {
            return params.setModpackResults;
        },
        get setModpackSearching() {
            return params.setModpackSearching;
        }
    });
    $effect.pre(() => {
        void params.creating;
        void params.searchMode;
        void params.searchValue;
        void params.versionChannel;
        const items = params.navItemsRef.current;
        return untrack(() => {
            // Commit the new command set and its selection in the same render.
            const nextIndex = items.length > 0 ? 0 : -1;
            params.setNavIndex(nextIndex);
            params.navIndexRef.current = nextIndex;
            params.setHoverProfileId(null);
            params.setHoverModpackIdx(null);
            params.setHoverVersionKey(null);
        });
    });
    useMainCommandKeydown({
        get activeView() {
            return params.activeView;
        },
        get setActiveView() {
            return params.setActiveView;
        },
        get navDirRef() {
            return params.navDirRef;
        },
        get showAccounts() {
            return params.showAccounts;
        },
        get deleteConfirmId() {
            return params.deleteConfirmId;
        },
        get logoutConfirm() {
            return params.logoutConfirm;
        },
        get showAdvConfirm() {
            return params.showAdvConfirm;
        },
        get configProfileId() {
            return params.configProfileId;
        },
        get loginModalOpenRef() {
            return params.loginModalOpenRef;
        },
        get inputRef() {
            return params.inputRef;
        },
        get createInputRef() {
            return params.createInputRef;
        },
        get isMacOS() {
            return params.isMacOS;
        },
        get searchMode() {
            return params.searchMode;
        },
        get onToggleVersionChannel() {
            return params.onToggleVersionChannel;
        },
        get onCompleteSearchCommand() {
            return params.onCompleteSearchCommand;
        },
        get onExitSearchMode() {
            return params.onExitSearchMode;
        },
        get searchValue() {
            return params.searchValue;
        },
        get creating() {
            return params.creating;
        },
        get setCreating() {
            return params.setCreating;
        },
        get navIndexRef() {
            return params.navIndexRef;
        },
        get navItemsRef() {
            return params.navItemsRef;
        },
        get navElemsRef() {
            return params.navElemsRef;
        },
        get setNavIndex() {
            return params.setNavIndex;
        },
        get setHoverProfileId() {
            return params.setHoverProfileId;
        },
        get setHoverModpackIdx() {
            return params.setHoverModpackIdx;
        },
        get setHoverVersionKey() {
            return params.setHoverVersionKey;
        },
        get hoverProfileId() {
            return params.hoverProfileId;
        },
        get hoverVersionKey() {
            return params.hoverVersionKey;
        },
        get profiles() {
            return params.profiles;
        },
        get activeProfileId() {
            return params.activeProfileId;
        },
        get setActiveProfileId() {
            return params.setActiveProfileId;
        },
        get savedAuth() {
            return params.savedAuth;
        },
        get manifest() {
            return params.manifest;
        },
        get modpackResults() {
            return params.modpackResults;
        },
        get openModpackVersionDialog() {
            return params.openModpackVersionDialog;
        },
        get versionDialogModpack() {
            return params.versionDialogModpack;
        },
        get modpackVersionsCache() {
            return params.modpackVersionsCache;
        },
        get modpackVersionIdx() {
            return params.modpackVersionIdx;
        },
        get setModpackVersionIdx() {
            return params.setModpackVersionIdx;
        },
        get installingModpackVersion() {
            return params.installingModpackVersion;
        },
        get setConfigProfileId() {
            return params.setConfigProfileId;
        },
        get setModsProfileId() {
            return params.setModsProfileId;
        },
        get handleDeleteProfile() {
            return params.handleDeleteProfile;
        },
        get isProfileBusy() {
            return params.isProfileBusy;
        },
        get handleLaunchGame() {
            return params.handleLaunchGame;
        },
        get handleWebviewLogin() {
            return params.handleWebviewLogin;
        }
    });
}
