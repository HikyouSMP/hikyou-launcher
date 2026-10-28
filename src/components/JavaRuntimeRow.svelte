<script lang="ts">
  import { onDestroy, tick, untrack } from "svelte";
  import { FolderOpen, LoaderCircle } from "@lucide/svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { createTranslation } from "../lib/i18n.svelte";
  import { managedJavaName, type JavaInstallation, type JavaRuntimeChoice } from "../javaRuntimes";
  let { major, override, managed, legacyPath, onChange }: {
    major: number; override?: string; managed?: JavaInstallation; legacyPath?: string;
    onChange: (major: number, path: string) => void;
  } = $props();
  const { t } = createTranslation();
  let draft = $state("");
  let busy = $state(false);
  let browsing = $state(false);
  let pending = Promise.resolve();
  let error = $state("");
  let choice = $state<JavaRuntimeChoice | null>(null);
  let input: HTMLInputElement;
  let active = true;
  onDestroy(() => { active = false; });
  const savedPath = $derived(override ?? legacyPath ?? "");
  const usingLegacy = $derived(override === undefined && !!legacyPath);
  $effect(() => { const path = savedPath; untrack(() => { draft = path; error = ""; if (choice?.path !== path) choice = null; }); });
  const dirty = $derived(draft.trim() !== savedPath);
  function reset() { draft = ""; error = ""; choice = null; onChange(major, ""); }
  function commit() { if (!busy) pending = select(false); }
  async function select(browse: boolean) {
    if (busy) return;
    if (!browse && !dirty) return;
    if (!browse && !draft.trim()) { reset(); return; }
    busy = true;
    browsing = browse;
    error = "";
    try {
      await tick();
      const result = browse
        ? await invoke<JavaRuntimeChoice | null>("pick_java_runtime", { major })
        : await invoke<JavaRuntimeChoice>("validate_java_runtime", { major, path: draft.trim() });
      // A blur-triggered save must also finish when navigating out of Settings.
      if (result && (active || !browse)) onChange(major, result.path);
      if (active && result) { choice = result; draft = result.path; }
    } catch (reason) { if (active) error = String(reason); }
    finally {
      if (active) {
        busy = false;
        browsing = false;
        if (browse) { await tick(); if (active) input?.focus(); }
      }
    }
  }
</script>

<section class="java-choice-row" aria-label={`Java ${major}`} aria-busy={busy} data-testid={`java-choice-${major}`} data-native-dialog-open={busy ? "" : undefined}>
  <div class="java-choice-heading">
    <strong class="text-sm font-medium text-t1">Java {major}</strong>
    <span class="text-xs text-t2">{usingLegacy ? t("settings.java_legacy_override") : override ? t("settings.java_custom") : t("settings.java_automatic")}</span>
  </div>
  <div class="min-w-0">
    <div class="java-choice-controls">
      <input class="glass-input min-w-0 w-full rounded-md px-3 py-2 text-xs text-t1 font-mono outline-none"
        bind:this={input} aria-label={t("settings.java_location", { major })} value={draft} disabled={busy}
        placeholder={t("settings.java_path_placeholder")}
        title={t(savedPath ? "settings.java_custom_detail" : "settings.java_automatic_detail")}
        oninput={(event) => { draft = event.currentTarget.value; error = ""; }}
        onblur={commit}
        onkeydown={(event) => {
          if (event.key === "Enter") { event.preventDefault(); commit(); }
          if (event.key === "Escape" && dirty) { event.stopPropagation(); draft = savedPath; error = ""; }
        }} />
      <button type="button" class="java-choice-action" disabled={browsing} onclick={async () => { await pending; if (active) void select(true); }}
        aria-label={t("settings.java_browse", { major })} title={t("settings.java_browse", { major })}>
        {#if browsing}<LoaderCircle size={17} class="animate-spin" />{:else}<FolderOpen size={17} />{/if}
      </button>
    </div>
    <div class="mt-1.5 text-xs text-t2 break-words">
      {#if override}{choice ? `${choice.vendor} · ${choice.javaVersion}` : t("settings.java_custom")}
      {:else if usingLegacy}{t("settings.java_legacy_override")}
      {:else if managed}{managedJavaName(managed)} · {managed.javaVersion}
      {:else}Azul Zulu · {t("settings.java_on_demand")}{/if}
    </div>
    {#if error}<p role="alert" class="mt-1.5 text-xs text-error break-words">{error}</p>{/if}
  </div>
</section>

<style>
  .java-choice-row { display: grid; grid-template-columns: 92px minmax(0, 1fr); gap: 12px; padding: 14px; }
  .java-choice-heading { display: flex; flex-direction: column; gap: 4px; padding-top: 6px; }
  .java-choice-controls { display: grid; grid-template-columns: minmax(0, 1fr) 32px; gap: 4px; align-items: center; }
  .java-choice-action { width: 32px; height: 36px; display: inline-flex; align-items: center; justify-content: center; color: var(--color-t2); border-radius: 6px; cursor: pointer; }
  .java-choice-action:hover:not(:disabled) { background: var(--color-hover); color: var(--color-t1); }
  .java-choice-action:focus-visible { outline: 2px solid var(--color-green); outline-offset: 2px; }
  .java-choice-action:disabled { opacity: .35; cursor: default; }
  @media (max-width: 560px) { .java-choice-row { grid-template-columns: minmax(0, 1fr); gap: 8px; } .java-choice-heading { flex-direction: row; align-items: baseline; gap: 12px; padding-top: 0; } }
</style>
