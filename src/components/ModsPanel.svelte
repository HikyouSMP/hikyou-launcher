<script lang="ts">
  import { RefreshCw } from "@lucide/svelte";
  import { createTranslation } from "../lib/i18n.svelte";
  import { css } from "../lib/dom";
  import { C } from "../theme";
  import type { ModSearchResult } from "../types";
  import { ModListView, type MLEntry } from "./ModListView";
  import ModRequestStatus from "./ModRequestStatus.svelte";
  import ModDecisions from "./ModDecisions.svelte";
  import ModFailureDetails from "./ModFailureDetails.svelte";
  import PlusPlaySetup from './PlusPlaySetup.svelte';
  import { toggleChoice } from "../modLifecycle";
  import { useModSnapshot } from "../hooks/useModSnapshot.svelte";

  interface Props {
    profileId: string;
    profileName: string;
    mcVersion: string;
    loader: string;
    onClose: () => void;
    embedded?: boolean;
  }
  let { profileId, mcVersion, loader, onClose, embedded = false }: Props = $props();
  const { t } = createTranslation();
  const model = useModSnapshot(() => profileId);
  const rows = $derived(model.snapshot?.rows ?? []);
  const blocked = $derived(model.busy || model.loading || model.loadFailed || !model.snapshot || (profileId.startsWith('smart:') && !model.snapshot.requested.intent.independent));
  const targetVersion = $derived(model.snapshot?.target?.mcVersion ?? mcVersion);
  const targetLoader = $derived(profileId.startsWith('smart:') ? 'fabric' : model.snapshot?.target?.loader ?? loader);
  const ldrColor = $derived(targetLoader === "fabric" ? C.fabric : targetLoader === "quilt" ? C.quilt : targetLoader === "forge" ? C.forge : targetLoader === "neoforge" ? C.neoforge : C.green);
  const ldrBg = $derived(targetLoader === "fabric" ? C.fabricBg : targetLoader === "quilt" ? C.quiltBg : targetLoader === "forge" ? C.forgeBg : targetLoader === "neoforge" ? C.neoforgeBg : C.greenBg);

  async function handleInstall(mod: ModSearchResult): Promise<void> {
    const row = rows.find((item) => item.projectId === mod.project_id);
    if (row) { await model.choose(row.key, "include"); return; }
    await model.install({ projectId: mod.project_id, mcVersion: targetVersion, loader: targetLoader, displayName: mod.title, iconUrl: mod.icon_url ?? null });
  }
  function handleToggle(key: string) {
    const row = rows.find((item) => item.key === key);
    const choice = row && toggleChoice(row);
    if (choice) void model.choose(key, choice);
  }
  function removeSearchResult(mod: ModSearchResult) {
    const row = rows.find((item) => item.projectId === mod.project_id);
    if (row) void model.choose(row.key, "exclude");
  }
  const entries = $derived<MLEntry[]>(rows.map((row) => ({
    id: row.key, projectId: row.projectId, name: row.name, icon: row.iconUrl,
    enabled: row.desired === "required" || (row.desired !== "excluded" && row.reason !== "legacyExcluded" && row.reason !== "defaultOff" && Boolean(row.active?.enabled)),
    subtitle: [t(`mod_lifecycle.reason_${row.reason}`), row.active?.version, row.active?.filename].filter(Boolean).join(" / "),
    canToggle: !blocked && toggleChoice(row) !== null,
    disabled: blocked,
    canDelete: row.canExclude,
    ...(row.canInherit ? { canInherit: true } : {}),
  })));
  function searchState(projectId: string): "idle" | "loading" | "done" {
    if (blocked) return "loading";
    const row = rows.find((item) => item.projectId === projectId);
    if (!row) return "idle";
    if (row.desired === "required" || (row.active?.enabled && row.desired !== "excluded" && row.reason !== "legacyExcluded")) return row.canExclude ? "done" : "loading";
    return row.canEnable ? "idle" : "loading";
  }
</script>

<ModListView {onClose} {embedded} {profileId} onHistoryRestored={() => model.refresh()} searchPlaceholder={t("mods.search_placeholder")} {entries} loading={model.loading && !model.snapshot} onEntryClick={handleToggle} onEntryDelete={(key) => { void model.choose(key, "exclude"); }} onEntryInherit={(key) => { void model.choose(key, "inherit"); }} deleteLabel={t("mod_lifecycle.exclude")} deleteConfirmation={t("mod_lifecycle.exclude_confirm")} loader={targetLoader} mcVersion={profileId.startsWith('smart:') ? '' : targetVersion} {searchState} onInstall={handleInstall} onSearchResultDelete={removeSearchResult}>
  {#snippet headerRight()}
    <span class="chip text-[10px] px-[6px] py-[1px] shrink-0 bg-surface text-t2" style="font-family: 'JetBrains Mono','SF Mono',monospace">{targetVersion}</span>
    <span class="chip text-[10px] px-[6px] py-[1px] font-semibold shrink-0" style={css({ background: ldrBg, color: ldrColor })}>{targetLoader}</span>
    <button onclick={() => model.retry()} disabled={model.busy} title={t("mods.refresh_btn")} class="icon-btn shrink-0"><RefreshCw size={13} /></button>
  {/snippet}
  {#snippet renderSearchMeta(mod)}
    {@const row = rows.find((item) => item.projectId === mod.project_id)}
    <div class="text-[10px] text-t3 mt-[1px]">
      {#if row}{t(`mod_lifecycle.reason_${row.reason}`)}{:else}↓ {mod.downloads.toLocaleString()}{/if}
    </div>
  {/snippet}
  {#snippet emptyNode()}
    {#if !model.loadFailed}
      <div class="py-8 px-4 text-center text-xs text-t3 leading-[1.6]">{t("mods.empty")}<br /><span class="text-[10px]">{t("mods.empty_hint")}</span></div>
    {/if}
  {/snippet}
  {#snippet errorNode()}
    <PlusPlaySetup {profileId} onComplete={()=>model.refresh()}/>
    {#if model.changeFailed || model.loadFailed || model.eventsFailed}
      <div role="alert" class="px-3 py-2 flex items-start gap-2 text-[11px] text-danger border-b border-b1">
        <span class="min-w-0 flex-1 break-words">{t(model.loadFailed ? "mod_lifecycle.load_failed" : model.eventsFailed ? "mod_lifecycle.events_failed" : "mod_lifecycle.change_failed")}</span>
        <button class="modal-btn shrink-0" disabled={model.busy} onclick={() => model.retry()}>{t("common.retry")}</button>
      </div>
    {/if}
    <ModFailureDetails failures={model.failures} />
    <ModRequestStatus state={model.snapshot?.requested ?? null} />
    <ModDecisions {profileId} panel />
  {/snippet}
</ModListView>
