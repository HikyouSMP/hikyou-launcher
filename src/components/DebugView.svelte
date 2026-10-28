<script lang="ts">
  import { ArrowLeft } from "@lucide/svelte";
  import { createTranslation } from "../lib/i18n.svelte";
  import { C } from "../theme";
  import type { AuthTokenDebugState, DebugInfo, DownloadProgress, GamePhase, LauncherSettings, Profile, SecurityMeasurement, StoredAuth } from "../types";
  import { DebugRow, DebugSection, Pill } from "./ui.ts";
  import { routeMotionClass } from "../utils/viewTransitions";
  import DebugLaunchMetrics, { formatMs, SLOW_TOTAL_MS } from "./DebugLaunchMetrics.svelte";
  import DebugJavaRuntimes from "./DebugJavaRuntimes.svelte";
  let { debugInfo, settings, savedAuth, profiles, profileRunStates, onBack }: {
    debugInfo: DebugInfo | null;
    settings: LauncherSettings;
    savedAuth: StoredAuth | null;
    profiles: Profile[];
    profileRunStates: Record<string, { phase: GamePhase; dlProgress: DownloadProgress | null }>;
    onBack: () => void;
  } = $props();
  const { t } = createTranslation();
  const storageBackend = $derived(debugInfo?.storageBackend ?? "");
  const storageDiagnostics = $derived(debugInfo?.storageDiagnostics);
  const usesPlatformCrypto = $derived(storageDiagnostics?.provider === "Microsoft Platform Crypto Provider");
  const verifiedPlatformCrypto = $derived(usesPlatformCrypto && storageDiagnostics?.hardwareBacking.status === "verified");
  const usesHardwareBackedStorage = $derived(verifiedPlatformCrypto || storageBackend.includes("Enclave") || storageBackend.includes("Keychain"));

  function storageMeasurementEntries(diagnostics: NonNullable<DebugInfo["storageDiagnostics"]>): Array<[string, SecurityMeasurement]> {
    return [
      ["hardwareBacking", diagnostics.hardwareBacking],
      ["keyScope", diagnostics.keyScope],
      ["exportPolicy", diagnostics.exportPolicy],
      ["userPresence", diagnostics.userPresence],
      ["pcrBinding", diagnostics.pcrBinding],
      ["accessControl", diagnostics.accessControl],
    ];
  }
  function tokenStateLabel(state: AuthTokenDebugState | undefined, translate: (key: string) => string) {
    if (!state) return translate("common.fetching");
    if (!state.available) return translate("debug.token_not_available");
    if (state.managed_by_os) return translate("debug.token_os_broker");
    return state.persisted ? translate("debug.token_encrypted_persistent") : translate("debug.token_transient_only");
  }
</script>

{#snippet measurementRow(label: string, measurement: SecurityMeasurement)}
  <DebugRow k={label} v={t(`debug.storage_status_${measurement.status}`)}
    highlight={measurement.status === "verified" || measurement.status === "enabled"}
    warn={measurement.status === "warning" || measurement.status === "unavailable"} />
{/snippet}

<div class={routeMotionClass("from-right") + " flex-1 flex flex-col overflow-hidden"} style:background="transparent">
  <div data-tauri-drag-region class="flex items-center gap-2 px-4 h-14 shrink-0 border-b border-b1">
    <Pill onClick={onBack} title={t("common.back")}><ArrowLeft size={14} /></Pill>
    <span class="text-sm font-normal text-t1 tracking-[-0.02em]">DEBUG</span>
  </div>
  <div class="sb flex-1 overflow-y-auto" style:font-family="'JetBrains Mono','SF Mono','Fira Code',monospace">
    <div class="p-4 flex flex-col gap-3.5">
      <DebugSection title={t("debug.section_java")}>
        <DebugRow k={t("debug.java_dist")} v={debugInfo?.javaVersion ?? t("debug.java_not_installed")} />
        <DebugRow k={t("debug.java_path")} v={debugInfo?.javaPath ?? "—"} mono />
        <DebugRow k={t("debug.java_memory")} v={debugInfo?.memoryMb ? `${debugInfo.memoryMb} MB` : t("debug.java_memory_val", { mb: settings.game.memoryMb })} />
        <DebugRow k={t("debug.java_last_profile")} v={debugInfo?.lastProfileId ?? t("debug.java_last_profile_pending")} />
        <DebugRow k={t("debug.jvm_tuning")} v={debugInfo?.jvmTuningMode === "performance_lab" ? t("debug.jvm_tuning_lab") : t("debug.jvm_tuning_smooth")} />
        {#if debugInfo}
          <div class="mt-1.5 mb-0.5 px-2 py-1.25 rounded-md text-[10px] leading-normal text-green bg-green-bg">
            {debugInfo.useZgc ? t("debug.zgc_mode") : t("debug.g1gc_mode")}
          </div>
        {/if}
        {#if debugInfo?.jvmArgs?.length}
          <details class="mt-1">
            <summary class="text-[10px] text-t3 cursor-pointer select-none py-0.75 list-none">{t("debug.jvm_flags_count", { count: debugInfo.jvmArgs.length })}</summary>
            <div class="mt-1.5 flex flex-col gap-0.5">
              {#each debugInfo.jvmArgs as arg}<span class="text-[10px] text-green rounded-md px-1.5 py-0.5 font-[inherit] leading-[1.6] bg-[rgba(0,0,0,.4)]">{arg}</span>{/each}
            </div>
          </details>
        {/if}
      </DebugSection>
      <DebugJavaRuntimes memoryMb={settings.game.memoryMb || 2048} />
      <DebugSection title={t("debug.section_launch_metrics")}>
        {#if debugInfo?.launchMetrics}<DebugLaunchMetrics metrics={debugInfo.launchMetrics} />
        {:else}<span class="text-[11px] text-t3">{t("debug.launch_metrics_pending")}</span>{/if}
      </DebugSection>
      <DebugSection title={t("debug.section_game_milestones")}>
        {#if debugInfo?.gameMilestones?.length}
          <div class="debug-milestone-strip">{#each debugInfo.gameMilestones as milestone}
            <div class="debug-milestone"><span>{t(`debug.milestone_${milestone.name}`, { defaultValue: milestone.name })}</span><b>{formatMs(milestone.elapsedMs)}</b></div>
          {/each}</div>
        {:else}<span class="text-[11px] text-t3">{t("debug.game_milestones_pending")}</span>{/if}
      </DebugSection>
      <DebugSection title={t("debug.section_launch_history")}>
        {#if debugInfo?.launchMetricHistory?.length}
          <div class="debug-history-strip">{#each debugInfo.launchMetricHistory as item}
            <div class="debug-history-item"><span title={item.profileId}>{item.profileId}</span><b class={item.totalPreSpawnMs >= SLOW_TOTAL_MS ? "is-warn" : ""}>{formatMs(item.totalPreSpawnMs)}</b></div>
          {/each}</div>
        {:else}<span class="text-[11px] text-t3">{t("debug.launch_history_pending")}</span>{/if}
      </DebugSection>
      <DebugSection title={t("debug.section_paths")}>
        {#if debugInfo?.launcherPaths}
          {#each Object.entries(debugInfo.launcherPaths) as [key, value]}<DebugRow k={key} v={String(value)} mono copyable openable />{/each}
        {:else}<span class="text-[11px] text-t3">{t("debug.loading")}</span>{/if}
      </DebugSection>
      <DebugSection title={t("debug.section_profiles")}>
        {#if Object.keys(profileRunStates).length === 0}<span class="text-[11px] text-t3">{t("debug.no_running_profiles")}</span>
        {:else}
          {#each Object.entries(profileRunStates) as [profileId, state] (profileId)}
            {@const profile = profiles.find((item) => item.id === profileId)}
            <div class="mb-1.5">
              <DebugRow k={t("debug.profile_id")} v={profileId} mono />
              <DebugRow k={t("debug.profile_name")} v={profile?.name ?? t("debug.profile_name_unknown")} />
              <DebugRow k={t("debug.profile_phase")} v={state.phase} highlight={state.phase === "running"} warn={state.phase === "downloading"} />
              {#if state.dlProgress}<DebugRow k={t("debug.profile_progress")} v={`${state.dlProgress.completed} / ${state.dlProgress.total} (${state.dlProgress.phase})`} />{/if}
            </div>
          {/each}
        {/if}
      </DebugSection>
      <DebugSection title={t("debug.section_auth")}>
        <DebugRow k={t("debug.auth_username")} v={savedAuth?.username ?? t("debug.auth_not_logged_in")} />
        <DebugRow k="UUID" v={savedAuth?.uuid ?? "—"} mono />
        <DebugRow k={t("debug.auth_token_expiry")} v={savedAuth?.expires_at ? new Date(savedAuth.expires_at * 1000).toLocaleString() : "—"} />
        <DebugRow k={t("debug.token_minecraft_access")} v={tokenStateLabel(debugInfo?.authTokens?.minecraft_access, t)} highlight={debugInfo?.authTokens?.minecraft_access.available ?? false} />
        <DebugRow k={t("debug.token_microsoft_refresh")} v={tokenStateLabel(debugInfo?.authTokens?.microsoft_refresh, t)} highlight={debugInfo?.authTokens?.microsoft_refresh.available ?? false} />
        <DebugRow k={t("debug.token_microsoft_access")} v={t("debug.token_transient_only")} />
        <DebugRow k={t("debug.token_xbox_user")} v={t("debug.token_transient_only")} />
        <DebugRow k={t("debug.token_xsts")} v={t("debug.token_transient_only")} />
      </DebugSection>
      <DebugSection title={t("debug.section_secure_storage")}>
        <DebugRow k={t("debug.storage_backend")} v={debugInfo?.storageBackend ?? t("common.fetching")} highlight={usesHardwareBackedStorage} warn={usesPlatformCrypto && !verifiedPlatformCrypto} />
        {#if storageDiagnostics}
          {@render measurementRow(t("debug.storage_hardware_backing"), storageDiagnostics.hardwareBacking)}
          {@render measurementRow(t("debug.storage_key_scope"), storageDiagnostics.keyScope)}
          {@render measurementRow(t("debug.storage_export_policy"), storageDiagnostics.exportPolicy)}
          {@render measurementRow(t("debug.storage_user_presence"), storageDiagnostics.userPresence)}
          {@render measurementRow(t("debug.storage_pcr_binding"), storageDiagnostics.pcrBinding)}
          {@render measurementRow(t("debug.storage_access_control"), storageDiagnostics.accessControl)}
          <details class="mt-1">
            <summary class="text-[10px] text-t3 cursor-pointer select-none py-0.75 list-none">{t("debug.storage_measurement_evidence")}</summary>
            <div class="mt-1.5 flex flex-col gap-1 text-[9px] leading-[1.55] text-t3">{#each storageMeasurementEntries(storageDiagnostics) as [name, item]}
              <div><span class="text-t2">{name}</span>: {item.detail} {item.evidence}</div>
            {/each}</div>
          </details>
        {/if}
        {#if debugInfo?.storageBackend}
          <div class="mt-1.5 mb-0.5 px-2 py-1.25 rounded-md text-[10px] leading-normal"
            style:background={verifiedPlatformCrypto ? C.greenBg : "rgba(184,144,48,.08)"} style:color={verifiedPlatformCrypto ? C.green : C.warning}>
            {verifiedPlatformCrypto ? t("debug.tpm_protection") : usesPlatformCrypto ? t("debug.tpm_unverified") : storageBackend.includes("Secure Enclave") ? t("debug.enclave_protection") : storageBackend.includes("Keychain") ? t("debug.keychain_protection") : storageBackend.includes("DPAPI") ? t("debug.dpapi_fallback") : t("debug.insecure_storage")}
          </div>
        {/if}
      </DebugSection>
    </div>
  </div>
</div>
