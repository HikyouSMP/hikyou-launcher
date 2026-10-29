<script lang="ts">
  import { createTranslation } from "../lib/i18n.svelte";
  import { css } from "../lib/dom";
  import type { Ref, Setter } from "../lib/state";
  import { C } from "../theme";
  import type { CrashAnalysis, LoaderType, ModSearchResult, Profile, SmartProfileStatus, StoredAuth, VersionEntry, VersionChannel } from "../types";
  import type { SearchSuggestion, SearchSuggestionAction } from "../search/suggestions";
  import type { CreatingProfileDraft } from "./CreateCandidateList.svelte";
  import type { InstallingModpackVersion } from "./ModpackVersionDialog.svelte";
  import CrashToast from "./CrashToast.svelte";
  import CreateCandidateList from "./CreateCandidateList.svelte";
  import { EmptyProfileHint, NoProfileMatch } from "./ProfileEmptyStates";
  import GameErrorBanner from "./GameErrorBanner.svelte";
  import MainFooter from "./MainFooter.svelte";
  import MainHeader from "./MainHeader.svelte";
  import ModpackResultsList from "./ModpackResultsList.svelte";
  import ProfileCreatePanel from "./ProfileCreatePanel.svelte";
  import { ProfileRow, RunningProfileRow } from "./ProfileRows";
  import { mainViewMotion, routeMotionClass } from "../utils/viewTransitions";

  type ProfileMouseEvent = MouseEvent & { currentTarget: EventTarget & HTMLElement };
  type MainCommandViewProps = {
    navDirection: "forward" | "back" | "none";
    header: {
      inputRef: Ref<HTMLInputElement | null>;
      searchValue: string;
      searchMode: "profile" | "modpack";
      versionChannel: VersionChannel;
      debugVisible: boolean;
      activeDebug: boolean;
      canOpenLogInspector: boolean;
      onSearchChange: (value: string) => void;
      onOpenLogInspector: () => void;
      onToggleDebug: () => void;
      onOpenSettings: () => void;
      suggestion: SearchSuggestion | null;
      onSuggestionAction: (action: SearchSuggestionAction) => void;
    };
    crash: {
      toast: CrashAnalysis | null;
      profileName: string;
      feedbackOpen: boolean;
      onFeedbackOpenChange: (open: boolean) => void;
      onClose: () => void;
      onCopyReport: () => Promise<void>;
    };
    auth: { savedAuth: StoredAuth | null; authLoaded: boolean; pending?: boolean; onLogin: () => void | Promise<void> };
    modpacks: {
      results: ModSearchResult[];
      searching: boolean;
      hoverIndex: number | null;
      onHoverIndexChange: (index: number | null) => void;
      onOpenVersionDialog: (modpack: ModSearchResult) => void;
      installingVersion: InstallingModpackVersion | null;
    };
    navigation: { navItems: string[]; navIndex: number; navElemsRef: Ref<Map<string, HTMLElement>>; ctrlHeld: boolean };
    profiles: {
      matched: Profile[];
      isBusy: (profileId: string) => boolean;
      getPhase: (profileId: string) => string;
      getDownloadProgress: (profileId: string) => { completed: number; total: number } | null;
      hoverProfileId: string | null;
      onHoverProfileIdChange: (profileId: string | null) => void;
      onOpenContextMenu: (profile: Profile, event: ProfileMouseEvent) => void;
      optionsCopySourceId: string | null;
      optionsCopyPulse: { profileId: string; kind: "pick" | "drop"; x: number; y: number } | null;
      onOptionsCopyPick: (profile: Profile, event: ProfileMouseEvent) => void;
      onOptionsCopyDrop: (profile: Profile, event: ProfileMouseEvent) => void;
      onActivate: (profile: Profile) => void;
      onDelete: (profileId: string) => void;
      totalCount: number;
      smartStatuses: SmartProfileStatus[];
    };
    creation: {
      candidateVersions: VersionEntry[];
      creating: CreatingProfileDraft | null;
      setCreating: Setter<CreatingProfileDraft | null>;
      createInputRef: Ref<HTMLInputElement | null>;
      labelForLoader: (loader: LoaderType | string) => string;
      handleCreateProfile: (versionId: string, loader: LoaderType, name?: string) => Promise<Profile | null | undefined>;
      launchAfterCreate: boolean;
      loadersForCreate: LoaderType[];
      hoverVersionKey: string | null;
      onHoverVersionKeyChange: (key: string | null) => void;
      intentEmpty: boolean;
      searchHasNoMatch: boolean;
      gameError?: string;
      onClearGameError: () => void;
    };
    footer: { appVersion: string; accountsOpen: boolean; onVersionClick: () => void; onToggleAccounts: () => void; unlockRemaining: number | null;
      suggestion: SearchSuggestion | null; onSuggestionAction: (action: SearchSuggestionAction) => void };
  };
  let { navDirection, header, crash, auth, modpacks, navigation, profiles, creation, footer }: MainCommandViewProps = $props();
  const { t } = createTranslation();

  function smartStatusLabel(profile: Profile) {
    const status = profiles.smartStatuses.find((item) => item.id === profile.id);
    if (!status) return null;
    if (!status.sync) return t("debug.smart_never");
    if (status.sync.fresh) return t("debug.smart_fresh");
    if (status.sync.folder_changed) return t("debug.smart_changed");
    return t("debug.smart_stale");
  }
  async function submitCreation() {
    const snapshot = creation.creating;
    if (!snapshot) return;
    creation.setCreating(null);
    const profile = await creation.handleCreateProfile(snapshot.versionId, snapshot.loader, snapshot.inputName || undefined);
    if (profile && auth.savedAuth && creation.launchAfterCreate) profiles.onActivate(profile);
  }
</script>

<div class={routeMotionClass(mainViewMotion(navDirection)) + " flex flex-col flex-1 overflow-hidden"}>
  <MainHeader {...header} />
  {#if crash.toast}
    <CrashToast analysis={crash.toast} profileName={crash.profileName} feedbackOpen={crash.feedbackOpen} onFeedbackOpenChange={crash.onFeedbackOpenChange} onClose={crash.onClose} onCopyReport={crash.onCopyReport} />
  {/if}
  <div class="sb flex-1 overflow-y-auto overflow-x-hidden py-1.25 relative">
    {#if navigation.ctrlHeld}<div class="absolute top-0 right-0 bottom-0 w-14 pointer-events-none z-1" style={css({ background: "linear-gradient(to left, rgba(18,18,16,.34) 0%, transparent 100%)", animation: "slideInRight .15s cubic-bezier(.16,1,.3,1) forwards" })}></div>{/if}
    {#if auth.authLoaded && !auth.savedAuth}
      <button type="button" disabled={auth.pending} aria-busy={auth.pending} onclick={auth.onLogin}
        class="flex items-center gap-2 px-3 py-2 mx-1.5 mb-0.5 rounded-md border border-transparent cursor-pointer disabled:cursor-wait transition-[background] duration-80 text-left bg-transparent"
        onmouseenter={(event) => { event.currentTarget.style.background = C.hover; }} onmouseleave={(event) => { event.currentTarget.style.background = "transparent"; }}>
        <div class="w-7 h-7 rounded-md flex items-center justify-center shrink-0" style="background:rgba(59,130,246,.12);border:1px solid rgba(59,130,246,.18)"><span class="text-fabric text-[12px] font-bold">M</span></div>
        <div><p class="text-[12px] font-medium text-t1">{t(auth.pending ? "auth.logging_in" : "auth.login_microsoft")}</p></div>
      </button>
    {/if}
    {#if header.searchMode === "modpack"}
      <ModpackResultsList results={modpacks.results} searching={modpacks.searching} searchValue={header.searchValue} navItems={navigation.navItems} navIndex={navigation.navIndex} hoveredIndex={modpacks.hoverIndex} navElemsRef={navigation.navElemsRef} onHoverIndexChange={modpacks.onHoverIndexChange} onOpenVersionDialog={modpacks.onOpenVersionDialog} />
    {/if}
    {#if header.searchMode === "profile" && modpacks.installingVersion}
      <div class="mx-1.5 mb-0.5 px-3 py-1.75 flex items-center gap-2"><div class="w-1.25 h-1.25 rounded-full bg-green pulse shrink-0"></div><p class="text-[12px] text-t3 overflow-hidden text-ellipsis whitespace-nowrap flex-1">{t("modpack.installing", { title: modpacks.installingVersion.title ?? "Modpack" })}</p></div>
    {/if}
    {#if header.searchMode === "profile"}
      {#each profiles.matched as profile (profile.id)}
        {#if profiles.isBusy(profile.id)}
          {@const phase = profiles.getPhase(profile.id)}
          {@const progress = profiles.getDownloadProgress(profile.id)}
          {@const percent = progress && progress.total > 0 ? Math.round((progress.completed / progress.total) * 100) : null}
          {@const statusText = phase === "downloading" && percent !== null ? t("profile.status_preparing_pct", { pct: percent }) : phase === "downloading" ? t("profile.status_preparing") : phase === "launching" ? t("profile.status_launching") : t("profile.status_running")}
          <RunningProfileRow {profile} {statusText} smartStatus={smartStatusLabel(profile)} progressPercent={phase === "downloading" ? (percent ?? undefined) : phase === "launching" ? undefined : null}
            onContextMenu={(event) => profiles.onOpenContextMenu(profile, event)} optionsCopySource={profiles.optionsCopySourceId === profile.id} optionsCopyPulse={profiles.optionsCopyPulse?.profileId === profile.id ? profiles.optionsCopyPulse : null} onMiddlePick={profiles.onOptionsCopyPick} onMiddleDrop={profiles.onOptionsCopyDrop} />
        {:else}
          {@const navKey = `p:${profile.id}`}
          <ProfileRow {profile} focused={navigation.navItems[navigation.navIndex] === navKey} hovered={profiles.hoverProfileId === profile.id} ctrlHeld={navigation.ctrlHeld} ctrlIndex={navigation.navItems.indexOf(navKey)}
            optionsCopySource={profiles.optionsCopySourceId === profile.id} optionsCopyPulse={profiles.optionsCopyPulse?.profileId === profile.id ? profiles.optionsCopyPulse : null} smartStatus={smartStatusLabel(profile)}
            navRef={(element) => { if (element) navigation.navElemsRef.current.set(navKey, element); else navigation.navElemsRef.current.delete(navKey); }}
            onHoverChange={(hovered) => profiles.onHoverProfileIdChange(hovered ? profile.id : null)} onActivate={() => profiles.onActivate(profile)} onDelete={() => profiles.onDelete(profile.id)} onContextMenu={(event) => profiles.onOpenContextMenu(profile, event)} onMiddlePick={profiles.onOptionsCopyPick} onMiddleDrop={profiles.onOptionsCopyDrop} />
        {/if}
      {/each}
    {/if}
    {#if header.searchMode === "profile" && creation.gameError}<GameErrorBanner error={creation.gameError} onClose={creation.onClearGameError} />{/if}
    {#if header.searchMode === "profile" && profiles.matched.length > 0 && creation.candidateVersions.length > 0 && !creation.creating}<div style={css({ margin: "4px 12px", height: 1, background: C.b1 })}></div>{/if}
    {#if header.searchMode === "profile" && creation.creating}
      <ProfileCreatePanel creating={creation.creating} inputRef={creation.createInputRef} labelForLoader={creation.labelForLoader}
        namePlaceholder={t("profile.name_placeholder", { label: creation.labelForLoader(creation.creating.loader), version: creation.creating.versionId })}
        onChangeName={(value) => creation.setCreating((current) => current ? { ...current, inputName: value } : current)} onCancel={() => creation.setCreating(null)} onSubmit={submitCreation} />
    {/if}
    {#if header.searchMode === "profile" && !creation.creating}
      <CreateCandidateList versions={creation.candidateVersions} loaders={creation.loadersForCreate} navItems={navigation.navItems} navIndex={navigation.navIndex} hoverKey={creation.hoverVersionKey} navElemsRef={navigation.navElemsRef} createInputRef={creation.createInputRef} labelForLoader={creation.labelForLoader} onHoverKeyChange={creation.onHoverVersionKeyChange} onCreateDraft={creation.setCreating} />
    {/if}
    {#if header.searchMode === "profile" && creation.intentEmpty && profiles.totalCount === 0 && !creation.creating}<EmptyProfileHint />{/if}
    {#if header.searchMode === "profile" && creation.searchHasNoMatch && !creation.creating}<NoProfileMatch query={header.searchValue} />{/if}
    <div style="height:8px"></div>
  </div>
  <MainFooter {...footer} savedAuth={auth.savedAuth} authPending={auth.pending} onLogin={auth.onLogin} />
</div>
