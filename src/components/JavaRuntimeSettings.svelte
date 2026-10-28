<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { RefreshCw, LoaderCircle } from "@lucide/svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { createTranslation } from "../lib/i18n.svelte";
  import type { JavaInventory, JavaManualCheck } from "../javaRuntimes";
  import JavaRuntimeRow from "./JavaRuntimeRow.svelte";
  import SGroup from "./SGroup.svelte";
  let { overrides, memoryMb, legacyPath, onChange, onClearLegacy }: {
    overrides: Record<string, string>; memoryMb: number; legacyPath?: string;
    onChange: (major: number, path: string) => void; onClearLegacy: () => void;
  } = $props();
  const { t } = createTranslation();
  let inventory = $state<JavaInventory | null>(null);
  let error = $state("");
  let checking = $state(false);
  let outcome = $state<"done" | "deferred" | "empty" | null>(null);
  let failures = $state<string[]>([]);
  let active = true;
  let requestId = 0;
  onDestroy(() => { active = false; });
  async function checkUpdates() {
    if (checking) return;
    checking = true;
    outcome = null;
    failures = [];
    const id = ++requestId;
    try {
      const result = await invoke<JavaManualCheck>("check_java_updates");
      if (!active || id !== requestId) return;
      inventory = result.status;
      error = "";
      failures = result.failures;
      outcome = result.checked === 0 ? "empty" : result.status.checks.some(check => check.deferred && result.status.installations.some(runtime => runtime.major === check.major && !runtime.legacy)) ? "deferred" : "done";
    } catch (reason) { if (active && id === requestId) failures = [String(reason)]; }
    finally { if (active && id === requestId) checking = false; }
  }
  onMount(() => {
    let pending = false;
    const refresh = async () => {
      if (pending || checking) return;
      pending = true;
      const id = ++requestId;
      try {
        const status = await invoke<JavaInventory>("get_java_debug_info", { memoryMb });
        if (active && id === requestId) { inventory = status; error = ""; }
      } catch (reason) { if (active && id === requestId) error = String(reason); }
      finally { pending = false; }
    };
    void refresh();
    const timer = setInterval(() => void refresh(), 10000);
    return () => clearInterval(timer);
  });
</script>

<SGroup label={t("settings.java_group")}>
  {#snippet actions()}
    <button type="button" class="java-update-action" onclick={checkUpdates} disabled={checking} data-testid="java-check-updates">
      {#if checking}<LoaderCircle size={13} class="animate-spin" />{:else}<RefreshCw size={13} />{/if}
      {t(checking ? "settings.java_checking_updates" : "settings.java_check_updates")}
    </button>
  {/snippet}
  <div data-testid="java-settings">
    {#if failures.length}<p role="alert" class="px-3 pt-3 text-xs text-error break-words">{failures.join("; ")}</p>
    {:else if outcome}<p role="status" class="px-3 pt-3 text-xs text-t2">{t(`settings.java_check_${outcome}`)}</p>{/if}
    {#if legacyPath}
      <div class="p-3 text-xs text-warning break-all">
        <p>{t("settings.java_legacy_override")}: {legacyPath}</p>
        <button type="button" class="mt-2 text-t1 underline" onclick={onClearLegacy}>{t("settings.java_clear_legacy")}</button>
      </div>
    {/if}
    {#if error}<p role="alert" class="p-3 text-xs text-error break-words">{error}</p>{/if}
    {#each [25, 21, 17, 8] as major (major)}
      {@const managed = inventory?.installations.find((runtime) => runtime.major === major && runtime.selected)}
      <JavaRuntimeRow {major} override={overrides[major]} {managed} {onChange} {legacyPath} />
    {/each}
  </div>
</SGroup>

<style>
  .java-update-action { display: inline-flex; align-items: center; gap: 5px; padding: 3px 5px; border: 0; border-radius: 4px; background: transparent; color: var(--color-t2); font: inherit; font-size: 11px; cursor: pointer; }
  .java-update-action:hover:not(:disabled) { color: var(--color-t1); background: var(--color-hover); }
  .java-update-action:disabled { opacity: .6; cursor: default; }
  .java-update-action:focus-visible { outline: 1px solid var(--color-t2); outline-offset: 2px; }
</style>
