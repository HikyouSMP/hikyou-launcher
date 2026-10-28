<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { RefreshCw, X } from "@lucide/svelte";
  import { createTranslation } from "../lib/i18n.svelte";
  import { css } from "../lib/dom";
  import { C } from "../theme";
  import type { ModSearchResult } from "../types";
  import { ModListView, type MLEntry } from "./ModListView";
  import ModRequestStatus from "./ModRequestStatus.svelte";
  import { projectFile, projectRequirement, requestedKey, type IdentifiedModFile, type RequestedState } from "../modRequests";

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
  let installed = $state<IdentifiedModFile[]>([]);
  let requested = $state<RequestedState | null>(null);
  let loadingInstalled = $state(true);
  let mutation = $state<string | null>(null);
  let installErr = $state<string | null>(null);
  let loadErr = $state<string | null>(null);
  let request = 0;
  let scope = 0;

  const ldrColor = $derived(loader === "fabric" ? C.fabric : loader === "quilt" ? C.quilt : loader === "forge" ? C.forge : loader === "neoforge" ? C.neoforge : C.green);
  const ldrBg = $derived(loader === "fabric" ? C.fabricBg : loader === "quilt" ? C.quiltBg : loader === "forge" ? C.forgeBg : loader === "neoforge" ? C.neoforgeBg : C.greenBg);
  async function refresh(id = profileId, epoch = scope, backfill = false) {
    const current = ++request;
    try {
      const [mods, state] = await Promise.all([
        invoke<IdentifiedModFile[]>("get_profile_mods", { profileId: id }),
        invoke<RequestedState>("get_profile_mod_requests", { profileId: id }),
      ]);
      if (current !== request || epoch !== scope) return;
      if (state.intent.profileId !== id) throw new Error(t("mod_requests.profile_mismatch"));
      installed = mods; requested = state; loadErr = null;
      if (backfill) {
        try {
          const updated = await invoke<IdentifiedModFile[]>("backfill_mod_metadata", { profileId: id });
          if (current === request && epoch === scope) installed = updated;
        } catch { /* Cached metadata remains available. */ }
      }
    } catch (reason) {
      if (current === request && epoch === scope) loadErr = String(reason);
    } finally {
      if (current === request && epoch === scope) loadingInstalled = false;
    }
  }
  $effect(() => {
    const id = profileId;
    const epoch = ++scope;
    installed = []; requested = null; loadingInstalled = true;
    mutation = null; installErr = null; loadErr = null;
    let disposed = false;
    const stops: UnlistenFn[] = [];
    async function subscribe(name: string, optional = false) {
      try {
        const stop = await listen<{ profileId: string }>(name, ({ payload }) => {
          if (!disposed && epoch === scope && payload.profileId === id) void refresh(id, epoch);
        });
        if (disposed || epoch !== scope) stop(); else stops.push(stop);
      } catch (reason) {
        if (!optional && !disposed && epoch === scope) installErr = String(reason);
      }
    }
    void Promise.all([subscribe("mods://prepared"), subscribe("mods://requests-changed", true)])
      .then(() => { if (!disposed && epoch === scope) void refresh(id, epoch, true); });
    return () => { disposed = true; scope++; request++; stops.forEach((stop) => stop()); };
  });

  async function mutate(command: string, args: Record<string, unknown>, key: string): Promise<void> {
    if (mutation || loadingInstalled || !requested) return;
    const id = profileId;
    const epoch = scope;
    mutation = key; installErr = null;
    try {
      await invoke(command, { ...args, profileId: id });
    } catch (reason) {
      if (epoch === scope) installErr = String(reason);
    } finally {
      // Resolution can fail after saving the request. A helper result is not a file snapshot.
      if (epoch === scope) await refresh(id, epoch);
      if (epoch === scope) mutation = null;
    }
  }

  async function handleInstall(mod: ModSearchResult): Promise<void> {
    await mutate("install_modrinth_mod", {
      projectId: mod.project_id, mcVersion, loader, displayName: mod.title, iconUrl: mod.icon_url ?? null,
    }, `modrinth:${mod.project_id}`);
  }
  async function handleToggle(filename: string): Promise<void> {
    await mutate("toggle_profile_mod", { filename }, filename);
  }
  async function handleRemove(filename: string): Promise<void> {
    await mutate("remove_profile_mod", { filename }, filename);
  }
  async function handleRemoveRequest(key: string): Promise<void> {
    await mutate("remove_requested_mod", { requestedKey: key }, key);
  }
  function removeSearchResult(mod: ModSearchResult) {
    const requirement = projectRequirement(requested, mod.project_id);
    const key = requirement && requestedKey(requirement);
    if (key) void handleRemoveRequest(key);
  }
  async function handleHistoryRestored(): Promise<void> {
    installErr = null;
    await refresh();
  }
  const entries = $derived<MLEntry[]>(installed.map((mod) => {
    const disabled = mod.filename.endsWith(".disabled");
    const version = mod.version_number ?? mod.version_id;
    const subtitle = [version, mod.display_name ? mod.filename : null].filter(Boolean).join(" / ");
    return { id: mod.filename, name: mod.display_name || (disabled ? mod.filename.replace(/\.disabled$/, "") : mod.filename), icon: mod.icon_url, enabled: !disabled, subtitle: subtitle || undefined };
  }));
  function searchState(projectId: string): "idle" | "loading" | "done" {
    if (mutation || loadingInstalled || !requested) return "loading";
    return projectRequirement(requested, projectId) ? "done" : "idle";
  }
</script>

<ModListView {onClose} {embedded} {profileId} onHistoryRestored={handleHistoryRestored} searchPlaceholder={t("mods.search_placeholder")} {entries} loading={loadingInstalled} onEntryClick={handleToggle} onEntryDelete={handleRemove} {loader} {mcVersion} {searchState} onInstall={handleInstall} onSearchResultDelete={removeSearchResult}>
  {#snippet headerRight()}
    <span class="chip text-[10px] px-[6px] py-[1px] shrink-0 bg-surface text-t2" style="font-family: 'JetBrains Mono','SF Mono',monospace">{mcVersion}</span>
    <span class="chip text-[10px] px-[6px] py-[1px] font-semibold tracking-[0.02em] shrink-0" style={css({ background: ldrBg, color: ldrColor })}>{loader}</span>
    <button onclick={() => refresh()} title={t("mods.refresh_btn")} class="flex items-center px-[6px] py-1 rounded-[6px] cursor-pointer transition-colors duration-[120ms] bg-transparent border-0 text-t3" onmouseenter={(event) => event.currentTarget.style.color = C.t1} onmouseleave={(event) => event.currentTarget.style.color = C.t3}><RefreshCw size={13} /></button>
  {/snippet}
  {#snippet renderSearchMeta(mod)}
    {@const requirement = projectRequirement(requested, mod.project_id)}
    <div class="text-[10px] text-t3 mt-[1px]">
      {#if requirement}{t(requirement.enabled ? "mod_requests.requested" : "mod_requests.requested_disabled")}
      {:else if projectFile(installed, mod.project_id)}{t("mod_requests.current_config")}
      {:else}↓ {mod.downloads.toLocaleString()}{/if}
    </div>
  {/snippet}
  {#snippet errorNode()}
    {#if installErr || loadErr}
      <div role="alert" class="mx-[10px] my-[6px] px-[10px] py-[6px] rounded-[6px] flex items-center justify-between gap-2 text-[11px] bg-danger-bg border border-danger-bdr text-danger">
        <span class="min-w-0 flex-1 break-words">{[installErr, loadErr].filter(Boolean).join(" / ")}</span>
        <button onclick={() => { installErr = null; loadErr = null; }} title={t("common.close")} class="cursor-pointer shrink-0 bg-transparent border-0 text-danger"><X size={13} /></button>
      </div>
    {/if}
    <ModRequestStatus state={requested} files={installed} busy={mutation !== null} onRemove={handleRemoveRequest} />
  {/snippet}
</ModListView>
