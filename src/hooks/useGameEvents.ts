import { useEffect, useRef } from "react";
import type { Dispatch, MutableRefObject, SetStateAction } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

import i18n from "../i18n";
import type {
  CrashAnalysis,
  DebugInfo,
  DownloadProgress,
  GamePhase,
  ParsedCrash,
  SmartProfileStatus,
} from "../types";

type ProfileRunStates = Record<
  string,
  { phase: GamePhase; dlProgress: DownloadProgress | null }
>;

interface UseGameEventsParams {
  activeProfileId: string | null;
  launchingProfileIdRef: MutableRefObject<string | null>;
  launchStartedAtRef: MutableRefObject<Map<string, number>>;
  profileLogs: Record<string, string[]>;
  setProfileLogs: Dispatch<SetStateAction<Record<string, string[]>>>;
  setProfileRunStates: Dispatch<SetStateAction<ProfileRunStates>>;
  setDebugInfo: Dispatch<SetStateAction<DebugInfo | null>>;
  setCrashAnalyses: Dispatch<SetStateAction<Record<string, CrashAnalysis>>>;
  setCrashToastProfileId: Dispatch<SetStateAction<string | null>>;
}

export function useGameEvents({
  activeProfileId,
  launchingProfileIdRef,
  launchStartedAtRef,
  profileLogs,
  setProfileLogs,
  setProfileRunStates,
  setDebugInfo,
  setCrashAnalyses,
  setCrashToastProfileId,
}: UseGameEventsParams) {
  const profileLogsRef = useRef<Record<string, string[]>>({});
  const seenMilestonesRef = useRef<Map<string, Set<string>>>(new Map());

  useEffect(() => {
    profileLogsRef.current = profileLogs;
  }, [profileLogs]);

  useEffect(() => {
    const unlisten = listen<{ version_id: string; profile_id: string }>(
      "game://launching",
      (event) => {
        const profileId = event.payload.profile_id;
        seenMilestonesRef.current.delete(profileId);
        setDebugInfo((prev) =>
          prev
            ? {
                ...prev,
                gameMilestones: (prev.gameMilestones ?? []).filter(
                  (milestone) => milestone.profileId !== profileId,
                ),
              }
            : prev,
        );
        setProfileRunStates((prev) => ({
          ...prev,
          [profileId]: { phase: "launching", dlProgress: null },
        }));
      },
    );

    return () => {
      unlisten.then((cleanup) => cleanup());
    };
  }, [setDebugInfo, setProfileRunStates]);

  useEffect(() => {
    const unlisten = listen<{
      profile_id: string;
      java_path: string;
      java_version: number;
      java_dist: string;
      is_liberica_nik: boolean;
      use_zgc: boolean;
      memory_max_mb: number;
      jvm_args: string[];
      jvm_flags_override?: string | null;
      jvm_tuning_mode?: "smooth" | "performance_lab" | null;
      jdk_override?: string | null;
    }>("debug://java-info", (event) => {
      setDebugInfo((prev) => ({
        launcherPaths: prev?.launcherPaths ?? {},
        javaPath: event.payload.java_path,
        javaVersion: `Java ${event.payload.java_version} — ${event.payload.java_dist}`,
        memoryMb: event.payload.memory_max_mb,
        isLiberica: event.payload.is_liberica_nik,
        useZgc: event.payload.use_zgc ?? false,
        jvmArgs: event.payload.jvm_args,
        lastProfileId: event.payload.profile_id,
        jvmFlagsOverride: event.payload.jvm_flags_override ?? null,
        jvmTuningMode: event.payload.jvm_tuning_mode ?? "smooth",
        jdkOverride: event.payload.jdk_override ?? null,
        storageBackend: prev?.storageBackend,
        launchMetrics: prev?.launchMetrics,
        launchMetricHistory: prev?.launchMetricHistory,
        gameMilestones: prev?.gameMilestones,
        smartProfileStatuses: prev?.smartProfileStatuses,
      }));
    });

    return () => {
      unlisten.then((cleanup) => cleanup());
    };
  }, [setDebugInfo]);

  useEffect(() => {
    const classifyMilestone = (line: string): string | null => {
      if (line.includes("Loading Minecraft")) return "loader_start";
      if (line.includes("Reloading ResourceManager")) return "resource_reload";
      if (line.includes("Loaded ") && line.includes(" recipes")) return "recipes_loaded";
      if (line.includes("Created:") && line.includes("textures/atlas")) return "atlas_created";
      if (line.includes("Stopping!")) return "normal_shutdown";
      return null;
    };

    const unlisten = listen<{ profile_id: string; line: string }>(
      "game://log",
      (event) => {
        const profileId = event.payload.profile_id;
        const milestone = classifyMilestone(event.payload.line);
        if (!milestone) return;
        const seen =
          seenMilestonesRef.current.get(profileId) ?? new Set<string>();
        if (seen.has(milestone)) return;
        seen.add(milestone);
        seenMilestonesRef.current.set(profileId, seen);
        const startedAt = launchStartedAtRef.current.get(profileId);
        if (!startedAt) return;
        setDebugInfo((prev) => {
          const next = [
            ...(prev?.gameMilestones ?? []),
            {
              profileId,
              name: milestone,
              elapsedMs: Date.now() - startedAt,
              line: event.payload.line,
            },
          ].slice(-12);
          return {
            ...(prev ?? {
              javaPath: "",
              javaVersion: "",
              launcherPaths: {},
            }),
            gameMilestones: next,
          };
        });
      },
    );

    return () => {
      unlisten.then((cleanup) => cleanup());
    };
  }, [launchStartedAtRef, setDebugInfo]);

  useEffect(() => {
    const unlisten = listen<{
      profile_id: string;
      version_id: string;
      total_pre_spawn_ms: number;
      java_spawn_ms: number;
      stages: Array<{ name: string; ms: number }>;
    }>("debug://launch-metrics", (event) => {
      const launchMetrics = {
        profileId: event.payload.profile_id,
        versionId: event.payload.version_id,
        createdAt: Math.floor(Date.now() / 1000),
        totalPreSpawnMs: event.payload.total_pre_spawn_ms,
        javaSpawnMs: event.payload.java_spawn_ms,
        stages: event.payload.stages,
      };
      invoke("record_launch_metrics", { metrics: launchMetrics }).catch((error) => {
        console.error("Failed to record launch metrics", error);
      });
      invoke<SmartProfileStatus[]>("get_smart_profile_statuses")
        .then((smartProfileStatuses) =>
          setDebugInfo((prev) => ({
            ...(prev ?? {
              javaPath: "",
              javaVersion: "",
              launcherPaths: {},
            }),
            smartProfileStatuses,
          })),
        )
        .catch(() => {});
      setDebugInfo((prev) => {
        const history = [
          launchMetrics,
          ...(prev?.launchMetricHistory ?? []).filter(
            (item) => item.profileId !== launchMetrics.profileId,
          ),
        ].slice(0, 10);
        return {
          ...(prev ?? {
            javaPath: "",
            javaVersion: "",
            launcherPaths: {},
          }),
          launchMetrics,
          launchMetricHistory: history,
        };
      });
    });

    return () => {
      unlisten.then((cleanup) => cleanup());
    };
  }, [setDebugInfo]);

  useEffect(() => {
    const unlisten = listen<{ exit_code: number | null; profile_id?: string }>(
      "game://exit",
      async (event) => {
        const profileId = event.payload.profile_id;

        if (!profileId) {
          setProfileRunStates({});
          return;
        }

        setProfileRunStates((prev) => {
          const next = { ...prev };
          delete next[profileId];
          return next;
        });

        if (event.payload.exit_code === 0 || event.payload.exit_code === null) {
          launchStartedAtRef.current.delete(profileId);
          return;
        }

        try {
          let analysis = await invoke<CrashAnalysis | null>(
            "get_latest_crash_analysis",
            {
              profileId,
              lang: i18n.language?.startsWith("en") ? "en" : "ja",
              sinceMs: launchStartedAtRef.current.get(profileId) ?? null,
            },
          );

          if (!analysis) {
            const liveLines = profileLogsRef.current[profileId] ?? [];
            if (liveLines.length > 0) {
              const parsed = await invoke<ParsedCrash>("parse_crash_log", {
                logLines: liveLines,
                lang: i18n.language?.startsWith("en") ? "en" : "ja",
              });
              if (
                parsed.is_crash_report ||
                parsed.rule_match != null ||
                parsed.exceptions.length > 0 ||
                parsed.diagnosis.confidence >= 0.5
              ) {
                analysis = {
                  profile_id: profileId,
                  source: "live_log",
                  source_path: null,
                  lines: liveLines,
                  parsed,
                };
              }
            }
          }

          if (!analysis) return;

          const shouldOpenCrash =
            event.payload.exit_code !== 0 ||
            analysis.parsed.is_crash_report ||
            analysis.parsed.rule_match != null ||
            analysis.parsed.exceptions.length > 0;

          if (!shouldOpenCrash) return;

          setCrashAnalyses((prev) => ({ ...prev, [profileId]: analysis }));
          setCrashToastProfileId(profileId);
          const window = getCurrentWindow();
          await window.show().catch(() => {});
          await window.setFocus().catch(() => {});
        } catch (error) {
          console.error("Failed to analyze Minecraft crash", error);
        } finally {
          launchStartedAtRef.current.delete(profileId);
        }
      },
    );

    return () => {
      unlisten.then((cleanup) => cleanup());
    };
  }, [
    launchStartedAtRef,
    setCrashAnalyses,
    setCrashToastProfileId,
    setProfileRunStates,
  ]);

  useEffect(() => {
    const unlisten = listen<DownloadProgress & { profile_id?: string }>(
      "download://progress",
      (event) => {
        const profileId =
          event.payload.profile_id ??
          launchingProfileIdRef.current ??
          activeProfileId ??
          "__global";

        setProfileRunStates((prev) => ({
          ...prev,
          [profileId]: { phase: "downloading", dlProgress: event.payload },
        }));
      },
    );

    return () => {
      unlisten.then((cleanup) => cleanup());
    };
  }, [activeProfileId, launchingProfileIdRef, setProfileRunStates]);

  useEffect(() => {
    const unlisten = listen<{ profile_id: string; line: string } | string>(
      "game://log",
      (event) => {
        const profileId =
          typeof event.payload === "string"
            ? "__unknown"
            : (event.payload.profile_id ?? "__unknown");
        const line =
          typeof event.payload === "string" ? event.payload : event.payload.line;

        setProfileLogs((prev) => {
          const current = prev[profileId] ?? [];
          const next = [...current, line];
          return {
            ...prev,
            [profileId]: next.length > 2000 ? next.slice(-2000) : next,
          };
        });
      },
    );

    return () => {
      unlisten.then((cleanup) => cleanup());
    };
  }, [setProfileLogs]);
}
