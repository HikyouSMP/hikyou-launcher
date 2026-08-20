import type { MutableRefObject, RefObject } from "react";
import { useTranslation } from "react-i18next";

import { C } from "../theme";
import type {
  CrashAnalysis,
  LoaderType,
  ModSearchResult,
  Profile,
  SmartProfileStatus,
  StoredAuth,
  VersionEntry,
} from "../types";
import type { CreatingProfileDraft } from "./CreateCandidateList";
import type { InstallingModpackVersion } from "./ModpackVersionDialog";
import { CrashToast } from "./CrashToast";
import { CreateCandidateList } from "./CreateCandidateList";
import { EmptyProfileHint, NoProfileMatch } from "./ProfileEmptyStates";
import { GameErrorBanner } from "./GameErrorBanner";
import { MainFooter } from "./MainFooter";
import { MainHeader } from "./MainHeader";
import { ModpackResultsList } from "./ModpackResultsList";
import { ProfileCreatePanel } from "./ProfileCreatePanel";
import { ProfileRow, RunningProfileRow } from "./ProfileRows";

type MainCommandViewProps = {
  navDirection: "forward" | "back" | "none";
  header: {
    inputRef: RefObject<HTMLInputElement | null>;
    searchValue: string;
    searchMode: "profile" | "modpack";
    debugVisible: boolean;
    activeDebug: boolean;
    canOpenLogInspector: boolean;
    onSearchChange: (value: string) => void;
    onOpenLogInspector: () => void;
    onToggleDebug: () => void;
    onOpenSettings: () => void;
  };
  crash: {
    toast: CrashAnalysis | null;
    profileName: string;
    feedbackOpen: boolean;
    onFeedbackOpenChange: (open: boolean) => void;
    onClose: () => void;
    onCopyReport: () => Promise<void>;
  };
  auth: {
    savedAuth: StoredAuth | null;
    authLoaded: boolean;
    onLogin: () => void | Promise<void>;
  };
  modpacks: {
    results: ModSearchResult[];
    searching: boolean;
    hoverIndex: number | null;
    onHoverIndexChange: (index: number | null) => void;
    onOpenVersionDialog: (modpack: ModSearchResult) => void;
    installingVersion: InstallingModpackVersion | null;
  };
  navigation: {
    navItems: string[];
    navIndex: number;
    navElemsRef: MutableRefObject<Map<string, HTMLElement>>;
    ctrlHeld: boolean;
  };
  profiles: {
    matched: Profile[];
    isBusy: (profileId: string) => boolean;
    getPhase: (profileId: string) => string;
    getDownloadProgress: (profileId: string) => { completed: number; total: number } | null;
    hoverProfileId: string | null;
    onHoverProfileIdChange: (profileId: string | null) => void;
    onOpenContextMenu: (profile: Profile, event: React.MouseEvent) => void;
    optionsCopySourceId: string | null;
    optionsCopyPulse: {
      profileId: string;
      kind: "pick" | "drop";
      x: number;
      y: number;
    } | null;
    onOptionsCopyPick: (profile: Profile, event: React.MouseEvent<HTMLElement>) => void;
    onOptionsCopyDrop: (profile: Profile, event: React.MouseEvent<HTMLElement>) => void;
    onActivate: (profile: Profile) => void;
    onDelete: (profileId: string) => void;
    totalCount: number;
    smartStatuses: SmartProfileStatus[];
  };
  creation: {
    candidateVersions: VersionEntry[];
    creating: CreatingProfileDraft | null;
    setCreating: React.Dispatch<React.SetStateAction<CreatingProfileDraft | null>>;
    createInputRef: RefObject<HTMLInputElement | null>;
    labelForLoader: (loader: LoaderType | string) => string;
    handleCreateProfile: (
      versionId: string,
      loader: LoaderType,
      name?: string,
    ) => Promise<Profile | null | undefined>;
    launchAfterCreate: boolean;
    loadersForCreate: LoaderType[];
    hoverVersionKey: string | null;
    onHoverVersionKeyChange: (key: string | null) => void;
    intentEmpty: boolean;
    searchHasNoMatch: boolean;
    gameError?: string;
    onClearGameError: () => void;
  };
  footer: {
    appVersion: string;
    accountsOpen: boolean;
    onVersionClick: () => void;
    onToggleAccounts: () => void;
  };
};

import { mainViewMotion, routeMotionClass } from "../utils/viewTransitions";

export function MainCommandView({
  navDirection,
  header,
  crash,
  auth,
  modpacks,
  navigation,
  profiles,
  creation,
  footer,
}: MainCommandViewProps) {
  const { t } = useTranslation();

  const {
    inputRef,
    searchValue,
    searchMode,
    debugVisible,
    activeDebug,
    canOpenLogInspector,
    onSearchChange,
    onOpenLogInspector,
    onToggleDebug,
    onOpenSettings,
  } = header;
  const {
    toast: crashToast,
    profileName: crashProfileName,
    feedbackOpen: crashFeedbackOpen,
    onFeedbackOpenChange: onCrashFeedbackOpenChange,
    onClose: onCloseCrashToast,
    onCopyReport: onCopyCrashReport,
  } = crash;
  const { savedAuth, authLoaded, onLogin } = auth;
  const {
    results: modpackResults,
    searching: modpackSearching,
    hoverIndex: hoverModpackIdx,
    onHoverIndexChange: onHoverModpackIdxChange,
    onOpenVersionDialog: onOpenModpackVersionDialog,
    installingVersion: installingModpackVersion,
  } = modpacks;
  const { navItems, navIndex, navElemsRef, ctrlHeld } = navigation;
  const {
    matched: matchedProfiles,
    isBusy: isProfileBusy,
    getPhase: getProfilePhase,
    getDownloadProgress: getProfileDlProgress,
    hoverProfileId,
    onHoverProfileIdChange,
    onOpenContextMenu: onOpenProfileContextMenu,
    optionsCopySourceId,
    optionsCopyPulse,
    onOptionsCopyPick,
    onOptionsCopyDrop,
    onActivate: onActivateProfile,
    onDelete: onDeleteProfile,
    totalCount: profilesLength,
    smartStatuses,
  } = profiles;
  const smartStatusForProfile = (profile: Profile) =>
    smartStatuses.find((status) => status.id === profile.id) ?? null;
  const smartStatusLabel = (status: SmartProfileStatus | null) => {
    if (!status) return null;
    if (!status.sync) return t("debug.smart_never");
    if (status.sync.fresh) return t("debug.smart_fresh");
    if (status.sync.folder_changed) return t("debug.smart_changed");
    return t("debug.smart_stale");
  };
  const {
    candidateVersions,
    creating,
    setCreating,
    createInputRef,
    labelForLoader,
    handleCreateProfile,
    launchAfterCreate,
    loadersForCreate,
    hoverVersionKey,
    onHoverVersionKeyChange,
    intentEmpty,
    searchHasNoMatch,
    gameError,
    onClearGameError,
  } = creation;
  const { appVersion, accountsOpen, onVersionClick, onToggleAccounts } = footer;

  return (
    <div
      className={
        routeMotionClass(mainViewMotion(navDirection)) +
        " flex flex-col flex-1 overflow-hidden"
      }
    >
      <MainHeader
        inputRef={inputRef}
        searchValue={searchValue}
        searchMode={searchMode}
        debugVisible={debugVisible}
        activeDebug={activeDebug}
        canOpenLogInspector={canOpenLogInspector}
        onSearchChange={onSearchChange}
        onOpenLogInspector={onOpenLogInspector}
        onToggleDebug={onToggleDebug}
        onOpenSettings={onOpenSettings}
      />

      {crashToast && (
        <CrashToast
          analysis={crashToast}
          profileName={crashProfileName}
          feedbackOpen={crashFeedbackOpen}
          onFeedbackOpenChange={onCrashFeedbackOpenChange}
          onClose={onCloseCrashToast}
          onCopyReport={onCopyCrashReport}
        />
      )}

      <div className="sb flex-1 overflow-y-auto overflow-x-hidden py-1.25 relative">
        {ctrlHeld && (
          <div
            className="absolute top-0 right-0 bottom-0 w-14 pointer-events-none z-1"
            style={{
              background:
                "linear-gradient(to left, rgba(18,18,16,.34) 0%, transparent 100%)",
              animation: "slideInRight .15s cubic-bezier(.16,1,.3,1) forwards",
            }}
          />
        )}

        {authLoaded && !savedAuth && (
          <div
            role="button"
            tabIndex={0}
            onClick={onLogin}
            onKeyDown={(event) => event.key === "Enter" && onLogin()}
            className="flex items-center gap-2 px-3 py-2 mx-1.5 mb-0.5 rounded-md border border-transparent cursor-pointer transition-[background] duration-80"
            onMouseEnter={(event) => (event.currentTarget.style.background = C.hover)}
            onMouseLeave={(event) =>
              (event.currentTarget.style.background = "transparent")
            }
          >
            <div
              className="w-7 h-7 rounded-md flex items-center justify-center shrink-0"
              style={{
                background: "rgba(59,130,246,.12)",
                border: "1px solid rgba(59,130,246,.18)",
              }}
            >
              <span className="text-fabric text-[12px] font-bold">M</span>
            </div>
            <div>
              <p className="text-[12px] font-medium text-t1">
                {t("auth.login_microsoft")}
              </p>
            </div>
          </div>
        )}

        {searchMode === "modpack" && (
          <ModpackResultsList
            results={modpackResults}
            searching={modpackSearching}
            searchValue={searchValue}
            navItems={navItems}
            navIndex={navIndex}
            hoveredIndex={hoverModpackIdx}
            navElemsRef={navElemsRef}
            onHoverIndexChange={onHoverModpackIdxChange}
            onOpenVersionDialog={onOpenModpackVersionDialog}
          />
        )}

        {searchMode === "profile" && installingModpackVersion && (
          <div className="mx-1.5 mb-0.5 px-3 py-1.75 flex items-center gap-2">
            <div className="w-1.25 h-1.25 rounded-full bg-green pulse shrink-0" />
            <p className="text-[12px] text-t3 overflow-hidden text-ellipsis whitespace-nowrap flex-1">
              {t("modpack.installing", {
                title: installingModpackVersion.title ?? "Modpack",
              })}
            </p>
          </div>
        )}

        {searchMode === "profile" &&
          matchedProfiles.map((profile) => {
            if (isProfileBusy(profile.id)) {
              const phase = getProfilePhase(profile.id);
              const progress = getProfileDlProgress(profile.id);
              const percent =
                progress && progress.total > 0
                  ? Math.round((progress.completed / progress.total) * 100)
                  : null;
              const statusText =
                phase === "downloading" && percent !== null
                  ? t("profile.status_preparing_pct", { pct: percent })
                  : phase === "downloading"
                    ? t("profile.status_preparing")
                    : phase === "launching"
                      ? t("profile.status_launching")
                      : t("profile.status_running");
              return (
                <RunningProfileRow
                  key={profile.id}
                  profile={profile}
                  statusText={statusText}
                  smartStatus={smartStatusLabel(smartStatusForProfile(profile))}
                  progressPercent={
                    phase === "downloading"
                      ? (percent ?? undefined)
                      : phase === "launching"
                        ? undefined
                        : null
                  }
                  onContextMenu={(event) => onOpenProfileContextMenu(profile, event)}
                  optionsCopySource={optionsCopySourceId === profile.id}
                  optionsCopyPulse={
                    optionsCopyPulse?.profileId === profile.id
                      ? optionsCopyPulse
                      : null
                  }
                  onMiddlePick={onOptionsCopyPick}
                  onMiddleDrop={onOptionsCopyDrop}
                />
              );
            }

            const navKey = `p:${profile.id}`;
            return (
              <ProfileRow
                key={profile.id}
                profile={profile}
                focused={navItems[navIndex] === navKey}
                hovered={hoverProfileId === profile.id}
                ctrlHeld={ctrlHeld}
                ctrlIndex={navItems.indexOf(navKey)}
                optionsCopySource={optionsCopySourceId === profile.id}
                optionsCopyPulse={
                  optionsCopyPulse?.profileId === profile.id
                    ? optionsCopyPulse
                    : null
                }
                smartStatus={smartStatusLabel(smartStatusForProfile(profile))}
                navRef={(element) => {
                  if (element) navElemsRef.current.set(navKey, element);
                  else navElemsRef.current.delete(navKey);
                }}
                onHoverChange={(hovered) =>
                  onHoverProfileIdChange(hovered ? profile.id : null)
                }
                onActivate={() => onActivateProfile(profile)}
                onDelete={() => onDeleteProfile(profile.id)}
                onContextMenu={(event) => onOpenProfileContextMenu(profile, event)}
                onMiddlePick={onOptionsCopyPick}
                onMiddleDrop={onOptionsCopyDrop}
              />
            );
          })}

        {searchMode === "profile" && gameError && (
          <GameErrorBanner error={gameError} onClose={onClearGameError} />
        )}

        {searchMode === "profile" &&
          matchedProfiles.length > 0 &&
          candidateVersions.length > 0 &&
          !creating && (
            <div style={{ margin: "4px 12px", height: 1, background: C.b1 }} />
          )}

        {searchMode === "profile" && creating && (
          <ProfileCreatePanel
            creating={creating}
            inputRef={createInputRef}
            labelForLoader={labelForLoader}
            namePlaceholder={t("profile.name_placeholder", {
              label: labelForLoader(creating.loader),
              version: creating.versionId,
            })}
            onChangeName={(value) =>
              setCreating((current) =>
                current ? { ...current, inputName: value } : current,
              )
            }
            onCancel={() => setCreating(null)}
            onSubmit={async () => {
              const snapshot = creating;
              setCreating(null);
              const profile = await handleCreateProfile(
                snapshot.versionId,
                snapshot.loader,
                snapshot.inputName || undefined,
              );
              if (profile && savedAuth && launchAfterCreate) {
                onActivateProfile(profile);
              }
            }}
          />
        )}

        {searchMode === "profile" && !creating && (
          <CreateCandidateList
            versions={candidateVersions}
            loaders={loadersForCreate}
            navItems={navItems}
            navIndex={navIndex}
            hoverKey={hoverVersionKey}
            navElemsRef={navElemsRef}
            createInputRef={createInputRef}
            labelForLoader={labelForLoader}
            onHoverKeyChange={onHoverVersionKeyChange}
            onCreateDraft={setCreating}
          />
        )}

        {searchMode === "profile" && intentEmpty && profilesLength === 0 && !creating && (
          <EmptyProfileHint />
        )}

        {searchMode === "profile" && searchHasNoMatch && !creating && (
          <NoProfileMatch query={searchValue} />
        )}

        <div style={{ height: 8 }} />
      </div>

      <MainFooter
        appVersion={appVersion}
        savedAuth={savedAuth}
        accountsOpen={accountsOpen}
        onVersionClick={onVersionClick}
        onToggleAccounts={onToggleAccounts}
        onLogin={onLogin}
      />
    </div>
  );
}
