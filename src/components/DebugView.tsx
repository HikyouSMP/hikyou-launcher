import { ArrowLeft } from "lucide-react";
import { useTranslation } from "react-i18next";

import { C } from "../theme";
import type {
  AuthTokenDebugState,
  DebugInfo,
  DownloadProgress,
  GamePhase,
  LaunchMetrics,
  LauncherSettings,
  Profile,
  StoredAuth,
} from "../types";
import { DebugRow, DebugSection, Pill } from "./ui";

type ProfileRunStates = Record<
  string,
  { phase: GamePhase; dlProgress: DownloadProgress | null }
>;

import { routeMotionClass } from "../utils/viewTransitions";

const SLOW_STAGE_MS = 2000;
const SLOW_TOTAL_MS = 5000;

function formatMs(ms: number) {
  if (ms >= 1000) return `${(ms / 1000).toFixed(ms >= 10000 ? 1 : 2)} s`;
  return `${ms} ms`;
}

export function DebugView({
  debugInfo,
  settings,
  savedAuth,
  profiles,
  profileRunStates,
  onBack,
}: {
  debugInfo: DebugInfo | null;
  settings: LauncherSettings;
  savedAuth: StoredAuth | null;
  profiles: Profile[];
  profileRunStates: ProfileRunStates;
  onBack: () => void;
}) {
  const { t } = useTranslation();
  const storageBackend = debugInfo?.storageBackend ?? "";
  const usesPlatformCrypto =
    storageBackend.includes("TPM") || storageBackend.includes("Platform Crypto Provider");
  const usesHardwareBackedStorage =
    usesPlatformCrypto ||
    storageBackend.includes("Enclave") ||
    storageBackend.includes("Keychain");

  return (
    <div
      className={routeMotionClass("from-right") + " flex-1 flex flex-col overflow-hidden"}
      style={{ background: "transparent" }}
    >
      <div
        data-tauri-drag-region
        className="flex items-center gap-2 px-4 h-14 shrink-0 border-b border-b1"
      >
        <Pill onClick={onBack} title={t("common.back")}>
          <ArrowLeft size={14} />
        </Pill>
        <span className="text-sm font-normal text-t1 tracking-[-0.02em]">
          DEBUG
        </span>
      </div>

      <div
        className="sb flex-1 overflow-y-auto"
        style={{ fontFamily: "'JetBrains Mono','SF Mono','Fira Code',monospace" }}
      >
        <div className="p-4 flex flex-col gap-3.5">
          <DebugSection title={t("debug.section_java")}>
            <DebugRow
              k={t("debug.java_dist")}
              v={debugInfo?.javaVersion ?? t("debug.java_not_installed")}
              highlight={!!debugInfo?.isLiberica}
            />
            <DebugRow k={t("debug.java_path")} v={debugInfo?.javaPath ?? "—"} mono />
            <DebugRow
              k={t("debug.java_memory")}
              v={
                debugInfo?.memoryMb
                  ? `${debugInfo.memoryMb} MB`
                  : t("debug.java_memory_val", { mb: settings.game.memoryMb })
              }
            />
            <DebugRow
              k={t("debug.java_last_profile")}
              v={debugInfo?.lastProfileId ?? t("debug.java_last_profile_pending")}
            />
            <DebugRow
              k={t("debug.jvm_tuning")}
              v={
                debugInfo?.jvmTuningMode === "performance_lab"
                  ? t("debug.jvm_tuning_lab")
                  : t("debug.jvm_tuning_smooth")
              }
            />
            {debugInfo != null && (
              <div className="mt-1.5 mb-0.5 px-2 py-1.25 rounded-md text-[10px] leading-normal text-green bg-green-bg">
                {debugInfo.useZgc
                  ? t("debug.zgc_mode")
                  : debugInfo.isLiberica
                    ? t("debug.g1gc_liberica")
                    : t("debug.g1gc_zulu")}
                {debugInfo.isLiberica && (
                  <span className="text-t3">{t("debug.graalvm_enabled")}</span>
                )}
              </div>
            )}
            {(debugInfo?.jvmArgs?.length ?? 0) > 0 && (
              <details className="mt-1">
                <summary className="text-[10px] text-t3 cursor-pointer select-none py-0.75 list-none">
                  {t("debug.jvm_flags_count", {
                    count: debugInfo!.jvmArgs!.length,
                  })}
                </summary>
                <div className="mt-1.5 flex flex-col gap-0.5">
                  {debugInfo!.jvmArgs!.map((arg, index) => (
                    <span
                      key={index}
                      className="text-[10px] text-green rounded-md px-1.5 py-0.5 font-[inherit] leading-[1.6] bg-[rgba(0,0,0,.4)]"
                    >
                      {arg}
                    </span>
                  ))}
                </div>
              </details>
            )}
          </DebugSection>

          <DebugSection title={t("debug.section_launch_metrics")}>
            {debugInfo?.launchMetrics ? (
              <LaunchMetricsPanel metrics={debugInfo.launchMetrics} />
            ) : (
              <span className="text-[11px] text-t3">
                {t("debug.launch_metrics_pending")}
              </span>
            )}
          </DebugSection>

          <DebugSection title={t("debug.section_game_milestones")}>
            {(debugInfo?.gameMilestones?.length ?? 0) > 0 ? (
              <div className="debug-milestone-strip">
                {debugInfo!.gameMilestones!.map((milestone) => (
                  <div
                    className="debug-milestone"
                    key={`${milestone.profileId}:${milestone.name}:${milestone.elapsedMs}`}
                  >
                    <span>
                      {t(`debug.milestone_${milestone.name}`, {
                        defaultValue: milestone.name,
                      })}
                    </span>
                    <b>{formatMs(milestone.elapsedMs)}</b>
                  </div>
                ))}
              </div>
            ) : (
              <span className="text-[11px] text-t3">
                {t("debug.game_milestones_pending")}
              </span>
            )}
          </DebugSection>

          <DebugSection title={t("debug.section_launch_history")}>
            {(debugInfo?.launchMetricHistory?.length ?? 0) > 0 ? (
              <div className="debug-history-strip">
                {debugInfo!.launchMetricHistory!.map((item) => (
                  <div
                    className="debug-history-item"
                    key={`${item.profileId}:${item.versionId}:${item.totalPreSpawnMs}`}
                  >
                    <span title={item.profileId}>{item.profileId}</span>
                    <b
                      className={
                        item.totalPreSpawnMs >= SLOW_TOTAL_MS ? "is-warn" : ""
                      }
                    >
                      {formatMs(item.totalPreSpawnMs)}
                    </b>
                  </div>
                ))}
              </div>
            ) : (
              <span className="text-[11px] text-t3">
                {t("debug.launch_history_pending")}
              </span>
            )}
          </DebugSection>

          <DebugSection title={t("debug.section_paths")}>
            {debugInfo?.launcherPaths ? (
              Object.entries(debugInfo.launcherPaths).map(([key, value]) => (
                <DebugRow key={key} k={key} v={String(value)} mono copyable openable />
              ))
            ) : (
              <span className="text-[11px] text-t3">{t("debug.loading")}</span>
            )}
          </DebugSection>

          <DebugSection title={t("debug.section_profiles")}>
            {Object.keys(profileRunStates).length === 0 ? (
              <span className="text-[11px] text-t3">
                {t("debug.no_running_profiles")}
              </span>
            ) : (
              Object.entries(profileRunStates).map(([profileId, state]) => {
                const profile = profiles.find((item) => item.id === profileId);
                return (
                  <div key={profileId} className="mb-1.5">
                    <DebugRow k={t("debug.profile_id")} v={profileId} mono />
                    <DebugRow
                      k={t("debug.profile_name")}
                      v={profile?.name ?? t("debug.profile_name_unknown")}
                    />
                    <DebugRow
                      k={t("debug.profile_phase")}
                      v={state.phase}
                      highlight={state.phase === "running"}
                      warn={state.phase === "downloading"}
                    />
                    {state.dlProgress && (
                      <DebugRow
                        k={t("debug.profile_progress")}
                        v={`${state.dlProgress.completed} / ${state.dlProgress.total} (${state.dlProgress.phase})`}
                      />
                    )}
                  </div>
                );
              })
            )}
          </DebugSection>

          <DebugSection title={t("debug.section_auth")}>
            <DebugRow
              k={t("debug.auth_username")}
              v={savedAuth?.username ?? t("debug.auth_not_logged_in")}
            />
            <DebugRow k="UUID" v={savedAuth?.uuid ?? "—"} mono />
            <DebugRow
              k={t("debug.auth_token_expiry")}
              v={
                savedAuth?.expires_at
                  ? new Date(savedAuth.expires_at * 1000).toLocaleString()
                  : "—"
              }
            />
            <DebugRow
              k={t("debug.token_minecraft_access")}
              v={tokenStateLabel(debugInfo?.authTokens?.minecraft_access, t)}
              highlight={debugInfo?.authTokens?.minecraft_access.available ?? false}
            />
            <DebugRow
              k={t("debug.token_microsoft_refresh")}
              v={tokenStateLabel(debugInfo?.authTokens?.microsoft_refresh, t)}
              highlight={debugInfo?.authTokens?.microsoft_refresh.available ?? false}
            />
            <DebugRow
              k={t("debug.token_microsoft_access")}
              v={t("debug.token_transient_only")}
            />
            <DebugRow k={t("debug.token_xbox_user")} v={t("debug.token_transient_only")} />
            <DebugRow k={t("debug.token_xsts")} v={t("debug.token_transient_only")} />
          </DebugSection>

          <DebugSection title={t("debug.section_secure_storage")}>
            <DebugRow
              k={t("debug.storage_backend")}
              v={debugInfo?.storageBackend ?? t("common.fetching")}
              highlight={usesHardwareBackedStorage}
            />
            {debugInfo?.storageBackend && (
              <div
                className="mt-1.5 mb-0.5 px-2 py-1.25 rounded-md text-[10px] leading-normal"
                style={{
                  background: usesPlatformCrypto
                    ? C.greenBg
                    : "rgba(184,144,48,.08)",
                  color: usesPlatformCrypto
                    ? C.green
                    : C.warning,
                }}
              >
                {usesPlatformCrypto
                  ? t("debug.tpm_protection")
                  : storageBackend.includes("Secure Enclave")
                    ? t("debug.enclave_protection")
                    : storageBackend.includes("Keychain")
                      ? t("debug.keychain_protection")
                      : storageBackend.includes("DPAPI")
                        ? t("debug.dpapi_fallback")
                        : t("debug.insecure_storage")}
              </div>
            )}
          </DebugSection>
        </div>
      </div>
    </div>
  );
}

function tokenStateLabel(
  state: AuthTokenDebugState | undefined,
  t: (key: string) => string,
) {
  if (!state) return t("common.fetching");
  if (!state.available) return t("debug.token_not_available");
  return state.persisted
    ? t("debug.token_encrypted_persistent")
    : t("debug.token_transient_only");
}

function LaunchMetricsPanel({ metrics }: { metrics: LaunchMetrics }) {
  const { t } = useTranslation();
  const maxStageMs = Math.max(1, ...metrics.stages.map((stage) => stage.ms));
  const slowest = metrics.stages.reduce((current, stage) =>
    stage.ms > current.ms ? stage : current,
  );
  const stageRows = metrics.stages.filter((stage) => stage.ms > 0);

  return (
    <div className="debug-metrics">
      <div className="debug-metric-head">
        <div className="min-w-0">
          <div className="debug-metric-profile" title={metrics.profileId}>
            {metrics.profileId}
          </div>
          <div className="debug-metric-version">
            {t("debug.launch_version")} {metrics.versionId}
          </div>
        </div>
        <div className="debug-metric-total">
          <span>{t("debug.launch_total_pre_spawn")}</span>
          <b
            className={
              metrics.totalPreSpawnMs >= SLOW_TOTAL_MS ? "is-warn" : "is-good"
            }
          >
            {formatMs(metrics.totalPreSpawnMs)}
          </b>
        </div>
      </div>

      <div className="debug-metric-insights">
        <span>
          {t("debug.launch_slowest_stage")}
          <b className={slowest.ms >= SLOW_STAGE_MS ? "is-warn" : ""}>
            {t(`debug.launch_stage_${slowest.name}`, {
              defaultValue: slowest.name,
            })}{" "}
            {formatMs(slowest.ms)}
          </b>
        </span>
      </div>

      <div className="debug-stage-grid">
        {stageRows.map((stage) => {
          const ratio = Math.max(4, Math.round((stage.ms / maxStageMs) * 100));
          const warn = stage.ms >= SLOW_STAGE_MS;
          const isSlowest = stage.name === slowest.name;
          return (
            <div
              key={stage.name}
              className={`debug-stage-pill ${warn ? "is-warn" : ""} ${
                isSlowest ? "is-slowest" : ""
              }`}
              title={`${t(`debug.launch_stage_${stage.name}`, {
                defaultValue: stage.name,
              })}: ${formatMs(stage.ms)}`}
            >
              <div className="debug-stage-line">
                <span>
                  {t(`debug.launch_stage_${stage.name}`, {
                    defaultValue: stage.name,
                  })}
                </span>
                <b>{formatMs(stage.ms)}</b>
              </div>
              <div className="debug-stage-track">
                <i style={{ width: `${ratio}%` }} />
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
