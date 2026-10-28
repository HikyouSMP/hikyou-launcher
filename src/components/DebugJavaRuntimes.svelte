<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { createTranslation } from "../lib/i18n.svelte";
  import DebugSection from "./DebugSection.svelte";
  import { javaRuntimeState, managedJavaName, type JavaInventory } from "../javaRuntimes";
  let { memoryMb }: { memoryMb: number } = $props();
  let inventory = $state<JavaInventory | null>(null);
  let error = $state("");
  const groups = $derived.by(() => {
    if (!inventory) return [];
    const { installations, checks } = inventory;
    const majors = new Set([...installations.map((runtime) => runtime.major), ...checks.map((check) => check.major)]);
    return [...majors].sort((a, b) => b - a).map((major) => ({
      major,
      installations: installations.filter((runtime) => runtime.major === major),
      check: checks.find((check) => check.major === major),
    }));
  });
  const translation = createTranslation();
  const { t } = translation;
  const time = (seconds: number | null) => seconds === null ? t("debug.java_not_checked") : new Date(seconds * 1000).toLocaleString(translation.language);
  onMount(() => {
    let active = true;
    let pending = false;
    const refresh = async () => {
      if (pending) return;
      pending = true;
      try {
        const result = await invoke<JavaInventory>("get_java_debug_info", { memoryMb });
        if (active) { inventory = result; error = ""; }
      } catch (reason) {
        if (active) error = String(reason);
      } finally { pending = false; }
    };
    void refresh();
    const timer = setInterval(() => void refresh(), 10000);
    return () => { active = false; clearInterval(timer); };
  });
</script>

<DebugSection title={t("debug.section_java_inventory")}>
  {#if error}<p role="alert" class="text-xs text-error break-words">{error}</p>{/if}
  {#if inventory}
    <div data-testid="java-inventory" class="flex flex-col gap-3">
      {#each groups as group (group.major)}
        <section aria-label={`Java ${group.major}`} data-testid="java-runtime-group" data-java-major={group.major} class="java-runtime-group min-w-0">
          <div class="min-w-0">
          {#each group.installations as runtime (runtime.directory)}
            {@const state = javaRuntimeState(runtime)}
            <div data-testid="java-installation" data-state={state} class="java-installation min-w-0 mb-2">
              <div class="java-version flex flex-wrap items-baseline gap-x-3 gap-y-1 text-xs">
                <span class="text-t1 font-medium">Java {runtime.major}</span>
                <span class="text-t2 break-all">{runtime.javaVersion}</span>
                <span class="java-status" data-state={state} title={t(`debug.java_${state}_detail`)}>
                  {t(state === "invalid" ? "debug.java_invalid" : state === "excluded" ? "debug.java_legacy" : state === "default" ? "debug.java_selected" : "debug.java_retained")}
                </span>
              </div>
              <div class="mt-1 text-[11px]">
                <span class="java-source text-t3 break-all min-w-0">{managedJavaName(runtime)} · {runtime.directory}</span>
              </div>
              {#if runtime.error}<p class="text-xs text-warning mt-1 break-words">{runtime.error}</p>{/if}
            </div>
          {:else}
            <div class="flex flex-wrap items-baseline gap-x-3 gap-y-1 text-xs mb-1">
              <span class="text-t1 font-medium">Java {group.major}</span>
              <span class="java-status" data-state="excluded">{t("debug.java_not_installed")}</span>
            </div>
          {/each}
          </div>
          {#if group.check}
            {@const check = group.check}
            <div data-testid="java-update-check" class="text-[11px] text-t2 leading-relaxed min-w-0">
              <dl class="java-check-times">
                <dt>{t("debug.java_last_checked")}</dt><dd>{time(check.lastCheckedAt)}</dd>
                {#if !check.checking && check.nextCheckAt}<dt>{t("debug.java_next_check")}</dt><dd>{time(check.nextCheckAt)}</dd>{/if}
              </dl>
              {#if check.checking}<span class="java-status mt-1" role="status">{t("debug.java_checking")}</span>
              {:else}
                {#if check.deferred}<span class="java-status mt-1" role="status">{t("debug.java_update_deferred")}</span>{/if}
                {#if check.lastError}<p class="text-warning break-words" role="status">{check.lastError}</p>{/if}
              {/if}
              {#if check.cleanupError}<p class="text-warning break-words" role="status">{check.cleanupError}</p>{/if}
            </div>
          {/if}
        </section>
      {:else}<span class="text-xs text-t3">{t("debug.java_not_installed")}</span>{/each}
    </div>
  {:else if !error}<span class="text-xs text-t3">{t("common.fetching")}</span>{/if}
</DebugSection>

<style>
  .java-runtime-group { display: grid; grid-template-columns: minmax(0, 1fr) minmax(240px, 0.9fr); gap: 8px 24px; align-items: start; }
  .java-check-times { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 2px 12px; }
  .java-check-times dt { color: var(--color-t3); }
  .java-check-times dd { min-width: 0; overflow-wrap: anywhere; }
  .java-status { display: inline-flex; align-items: center; max-width: 100%; padding: 0 4px; border: 1px solid color-mix(in srgb, var(--color-t2) 25%, transparent); border-radius: 9999px; color: var(--color-t2); font-size: 10px; font-weight: 400; line-height: 1.3; overflow-wrap: anywhere; }
  .java-status[data-state="default"] { background: color-mix(in srgb, var(--color-t1) 6%, transparent); border-color: transparent; }
  .java-status[data-state="cleanup"] { border-style: dashed; border-color: color-mix(in srgb, var(--color-t2) 22%, transparent); }
  .java-status[data-state="invalid"] { color: var(--color-warning); border-color: color-mix(in srgb, var(--color-warning) 45%, transparent); }
  .java-installation[data-state="cleanup"] .java-version { opacity: 0.65; }
  .java-installation[data-state="cleanup"] .java-source { opacity: 0.7; }
  @media (max-width: 560px) { .java-runtime-group { grid-template-columns: minmax(0, 1fr); gap: 4px; } }
</style>
