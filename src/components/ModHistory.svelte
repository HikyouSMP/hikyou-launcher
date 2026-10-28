<script lang="ts">
  import { onMount } from "svelte";
  import { ArrowLeft, ArrowRight, RefreshCcw, RefreshCw, RotateCcw } from "@lucide/svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { createTranslation } from "../lib/i18n.svelte";
  import type { ModEntry, ModRevision, ModRevisionDiff, ModTarget } from "../types";

  let { profileId, onClose, onRestored }: {
    profileId: string;
    onClose: () => void;
    onRestored?: () => void | Promise<void>;
  } = $props();
  const translation = createTranslation();
  const { t } = translation;
  let history = $state<ModRevision[]>([]);
  let fromId = $state("");
  let toId = $state("");
  let diff = $state<ModRevisionDiff | null>(null);
  let loading = $state(true);
  let comparing = $state(false);
  let busy = $state(false);
  let confirm = $state(false);
  let error = $state<string | null>(null);
  let backButton = $state<HTMLButtonElement>();
  let confirmButton = $state<HTMLButtonElement>();
  let restoreButton = $state<HTMLButtonElement>();
  let request = 0;
  let comparison = 0;
  const selected = $derived(history.find((revision) => revision.id === toId));
  const smart = $derived(profileId.startsWith("smart:"));
  const short = (id: string) => id.slice(0, 10);
  const targetLabel = (target: ModTarget) => [target.mcVersion, target.loader, target.loaderVersion].filter(Boolean).join(" / ");
  function timestamp(value: string) {
    const date = new Date(value);
    return Number.isNaN(date.valueOf()) ? value : date.toLocaleString(translation.language);
  }
  function choose(id: string) { toId = id; confirm = false; }
  function cancelRestore() {
    confirm = false;
    requestAnimationFrame(() => restoreButton?.focus());
  }
  async function reload(preferred?: string) {
    const current = ++request;
    loading = true;
    error = null;
    comparison++;
    diff = null;
    try {
      const revisions = await invoke<ModRevision[]>("get_profile_mod_history", { profileId });
      if (current !== request) return;
      history = revisions;
      fromId = revisions[0]?.id ?? "";
      toId = preferred && revisions.some((revision) => revision.id === preferred) ? preferred : revisions[1]?.id ?? fromId;
    } catch (reason) { if (current === request) error = String(reason); }
    finally { if (current === request) loading = false; }
  }
  $effect(() => { void reload(); return () => { request++; comparison++; }; });
  $effect(() => {
    const from = fromId, to = toId, profile = profileId;
    const current = ++comparison;
    confirm = false;
    diff = null;
    if (loading || !from || !to) { comparing = false; return; }
    comparing = true;
    void invoke<ModRevisionDiff>("get_profile_mod_revision_diff", { profileId: profile, fromId: from, toId: to })
      .then((result) => { if (current === comparison) diff = result; })
      .catch((reason) => { if (current === comparison) error = String(reason); })
      .finally(() => { if (current === comparison) comparing = false; });
  });
  async function mutate(resume = false) {
    if (busy || (!resume && !selected)) return;
    const current = request;
    busy = true; error = null; confirm = false;
    try {
      const revision = await invoke<ModRevision>(resume ? "resume_profile_mod_updates" : "restore_profile_mod_revision",
        resume ? { profileId } : { profileId, revisionId: toId });
      if (current !== request) return;
      await onRestored?.();
      if (current !== request) return;
      await reload(revision.id);
    } catch (reason) { error = String(reason); }
    finally { busy = false; }
  }
  $effect(() => { if (confirm) confirmButton?.focus(); });
  function keydown(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    event.preventDefault();
    event.stopImmediatePropagation();
    if (busy) return;
    if (confirm) cancelRestore();
    else onClose();
  }
  onMount(() => {
    backButton?.focus();
    window.addEventListener("keydown", keydown, true);
    return () => window.removeEventListener("keydown", keydown, true);
  });
</script>

{#snippet entry(mod: ModEntry)}
  <div class="min-w-0 text-[11px] leading-relaxed">
    <div class="flex flex-wrap items-baseline gap-x-2">
      <span class="text-t1 break-all">{mod.displayName || mod.modId || mod.filename}</span>
      <span class="font-mono text-t2 break-all" title={[mod.providerVersionNumber, mod.providerVersion].filter(Boolean).join(" / ") || undefined}>{mod.version || mod.providerVersionNumber || t("mod_history.unknown_version")}</span>
      <span class="text-t3">{mod.enabled ? t("mod_history.enabled") : t("mod_history.disabled")}</span>
    </div>
    <div class="text-[11px] text-t3 break-all font-mono">{mod.filename}</div>
    <div class="text-[11px] text-t3 font-mono" title={mod.objectId} aria-label={mod.objectId} data-testid="mod-object-hash">{mod.objectId.slice(0, 12)}...{mod.objectId.slice(-8)}</div>
  </div>
{/snippet}

<section class="flex flex-1 min-h-0 flex-col overflow-hidden" aria-label={t("mod_history.title")} data-testid="mod-history">
  <div class="flex h-[52px] shrink-0 items-center gap-2 border-b border-b1 px-3">
    <button bind:this={backButton} class="icon-btn" title={t("common.back")} disabled={busy} onclick={onClose}><ArrowLeft size={14} /></button>
    <h2 class="min-w-0 flex-1 text-[13px] text-t1 font-medium">{t("mod_history.title")}</h2>
    {#if smart}<button class="icon-btn" title={t("mod_history.resume")} disabled={busy || loading} onclick={() => mutate(true)}><RefreshCw size={14} /></button>{/if}
    <button class="icon-btn" title={t("mods.refresh_btn")} disabled={busy || loading} onclick={() => reload()}><RefreshCcw size={14} /></button>
  </div>
  {#if error}<div role="alert" class="shrink-0 px-3 py-2 text-[11px] text-danger break-all">{error}</div>{/if}
  <div class="sb flex-1 overflow-y-auto p-3 space-y-3" aria-busy={loading || comparing || busy}>
    {#if loading}<div role="status" class="text-xs text-t3">{t("mods.loading")}</div>
    {:else if history.length === 0}<div class="text-xs text-t3">{t("mod_history.empty")}</div>
    {:else}
      <div class="divide-y divide-b1">
        {#each history as revision (revision.id)}
          <button class="w-full text-left px-2 py-2 rounded-[4px] disabled:opacity-50" class:bg-hover={toId === revision.id} disabled={busy} onclick={() => choose(revision.id)} aria-pressed={toId === revision.id} data-testid="mod-history-revision">
            <div class="flex flex-wrap gap-x-2 text-[11px] text-t1"><span>{targetLabel(revision.target)}</span><span class="text-t3">{t("mod_history.count", { count: revision.mods.length })}</span></div>
            <div class="flex flex-wrap gap-x-2 text-[10px] text-t3"><time datetime={revision.createdAt}>{timestamp(revision.createdAt)}</time><span class="font-mono" title={revision.id}>{short(revision.id)}</span></div>
          </button>
        {/each}
      </div>
      <div class="flex flex-wrap items-end gap-2">
        <label class="min-w-0 flex-1 text-[10px] text-t3">{t("mod_history.from")}
          <select class="mt-1 w-full min-w-0 bg-surface border border-b1 rounded-[4px] px-2 py-1 text-[11px] text-t1" bind:value={fromId} disabled={busy}>
            {#each history as revision (revision.id)}<option value={revision.id}>{short(revision.id)} · {timestamp(revision.createdAt)}</option>{/each}
          </select>
        </label>
        <ArrowRight size={13} class="text-t3 mb-2 shrink-0" />
        <label class="min-w-0 flex-1 text-[10px] text-t3">{t("mod_history.to")}
          <select class="mt-1 w-full min-w-0 bg-surface border border-b1 rounded-[4px] px-2 py-1 text-[11px] text-t1" bind:value={toId} disabled={busy}>
            {#each history as revision (revision.id)}<option value={revision.id}>{short(revision.id)} · {timestamp(revision.createdAt)}</option>{/each}
          </select>
        </label>
      </div>
      {#if comparing}<div role="status" class="text-xs text-t3">{t("mods.loading")}</div>
      {:else if diff}
        <div class="text-[11px] text-t2 break-all" data-testid="mod-history-target-diff">{targetLabel(diff.fromTarget)} → {targetLabel(diff.toTarget)}</div>
        <div class="space-y-3" data-testid="mod-history-diff">
          {#each ["added", "removed"] as kind}
            {@const items = kind === "added" ? diff.added : diff.removed}
            <section aria-label={t(`mod_history.${kind}`)}>
              <h3 class="text-[11px] text-t2 font-medium mb-1">{t(`mod_history.${kind}`)} ({items.length})</h3>
              {#each items as mod (mod.filename)}<div class="border-l border-b1 pl-2 mb-2">{@render entry(mod)}</div>{/each}
            </section>
          {/each}
          <section aria-label={t("mod_history.changed")}>
            <h3 class="text-[11px] text-t2 font-medium mb-1">{t("mod_history.changed")} ({diff.changed.length})</h3>
            {#each diff.changed as change (change.after.filename)}
              <div class="border-l border-b1 pl-2 mb-3 space-y-1">
                <div class="text-[9px] text-t3">{t("mod_history.from")}</div>{@render entry(change.before)}
                <div class="text-[9px] text-t3">{t("mod_history.to")}</div>{@render entry(change.after)}
              </div>
            {/each}
          </section>
        </div>
      {/if}
      {#if selected}
        <section class="border-t border-b1 pt-3" aria-label={t("mod_history.restore")}>
          {#if confirm}
            <div role="alertdialog" aria-label={t("mod_history.confirm_title")} aria-describedby={smart ? "mod-history-confirm-body mod-history-world-warning" : "mod-history-confirm-body"} class="space-y-2">
              <div id="mod-history-confirm-body" class="text-[11px] text-t2 break-words">{t(smart ? "mod_history.confirm_body_smart" : "mod_history.confirm_body", { revision: short(selected.id) })}</div>
              {#if smart}<p id="mod-history-world-warning" class="text-[11px] text-t1 break-words">{t("mod_history.backup_worlds")}</p>{/if}
              <div class="flex flex-wrap gap-2">
                <button bind:this={confirmButton} class="text-[11px] text-t1 border border-b1 rounded-[4px] px-3 py-1.5" disabled={busy} onclick={() => mutate()}>{t("mod_history.confirm_restore")}</button>
                <button class="text-[11px] text-t3 px-2 py-1.5" onclick={cancelRestore}>{t("common.cancel")}</button>
              </div>
            </div>
          {:else}
            <button bind:this={restoreButton} class="flex items-center gap-2 text-[11px] text-t2 border border-b1 rounded-[4px] px-3 py-1.5 disabled:opacity-50" disabled={busy || comparing} onclick={() => confirm = true}><RotateCcw size={13} />{t("mod_history.restore")}</button>
          {/if}
          {#if busy}<div role="status" class="mt-2 text-[11px] text-t3">{t("mod_history.updating")}</div>{/if}
        </section>
        <section class="border-t border-b1 pt-3" aria-label={t("mod_history.mods")}>
          <h3 class="text-[11px] text-t2 font-medium mb-2">{t("mod_history.mods")} ({selected.mods.length})</h3>
          {#each selected.mods as mod (mod.filename)}<div class="mb-2">{@render entry(mod)}</div>{/each}
        </section>
      {/if}
    {/if}
  </div>
</section>
