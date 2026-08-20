// ─────────────────────────────────────────────────────────────────────────────
// Hikyou Launcher — 共通型定義
// ─────────────────────────────────────────────────────────────────────────────

export interface StoredAuth {
  expires_at: number;
  username?: string;
  uuid?: string;
}


export interface VersionEntry {
  id: string;
  type: string;
  url: string;
  time?: string;
  releaseTime?: string;
}

export interface VersionManifest {
  latest: { release: string; snapshot: string };
  versions: VersionEntry[];
}

export interface LoaderVersion {
  version: string;
  stable: boolean;
}

export type LoaderType = "vanilla" | "fabric" | "quilt" | "neoforge" | "forge";

export type ActiveView = "main" | "settings" | "debug" | "mods" | "rec-mods";


export type LoginState = "idle" | "waiting" | "success" | "error";
export type GamePhase = "idle" | "downloading" | "launching" | "running" | "error";

export interface DownloadProgress {
  completed: number;
  total: number;
  current_file?: string;
  bytes_downloaded?: number;
  bytes_total?: number;
  phase: string;
}

export interface CrashAction {
  kind: string;
  label: string;
  detail: string;
  target?: string | null;
}

export interface CrashDiagnosis {
  category: string;
  confidence: number;
  summary: string;
  evidence: string[];
  actions: CrashAction[];
}

export interface ParsedCrash {
  description?: string | null;
  exceptions: Array<{
    class: string;
    message?: string | null;
    top_frames: string[];
  }>;
  crash_mod?: string | null;
  mod_list: string[];
  mc_version?: string | null;
  java_version?: string | null;
  loader?: string | null;
  is_crash_report: boolean;
  rule_match?: { id: string; message: string } | null;
  diagnosis: CrashDiagnosis;
}

export interface CrashAnalysis {
  profile_id: string;
  source: "crash_report" | "latest_log" | string;
  source_path?: string | null;
  lines: string[];
  parsed: ParsedCrash;
}

export interface DebugInfo {
  javaPath: string;
  javaVersion: string;
  launcherPaths: Record<string, string>;
  memoryMb?: number;
  isLiberica?: boolean;
  useZgc?: boolean;
  jvmArgs?: string[];
  lastProfileId?: string;
  jvmFlagsOverride?: string | null;
  jvmTuningMode?: "smooth" | "performance_lab" | null;
  jdkOverride?: string | null;
  storageBackend?: string;
  authTokens?: AuthTokenDebugStatus;
  launchMetrics?: LaunchMetrics;
  launchMetricHistory?: LaunchMetrics[];
  gameMilestones?: GameMilestone[];
  smartProfileStatuses?: SmartProfileStatus[];
}

export interface AuthTokenDebugStatus {
  minecraft_access: AuthTokenDebugState;
  microsoft_refresh: AuthTokenDebugState;
  microsoft_access: AuthTokenDebugState;
  xbox_user: AuthTokenDebugState;
  xsts: AuthTokenDebugState;
}

export interface AuthTokenDebugState {
  persisted: boolean;
  available: boolean;
  expires_at: number | null;
}

export interface LaunchMetrics {
  id?: number | null;
  profileId: string;
  versionId: string;
  createdAt?: number | null;
  totalPreSpawnMs: number;
  javaSpawnMs: number;
  stages: Array<{ name: string; ms: number }>;
}

export interface GameMilestone {
  profileId: string;
  name: string;
  elapsedMs: number;
  line: string;
}

export interface SmartProfileStatus {
  id: string;
  name: string;
  game_dir: string;
  sync?: {
    mc_version: string;
    loader: string;
    synced_at: string;
    age_seconds: number;
    folder_changed: boolean;
    fresh: boolean;
  } | null;
}

export interface GameSettings {
  memoryMb: number;
  showSnapshots: boolean;
  launchAfterCreate: boolean;
  latestReleasePlus: boolean;
  snapshotPlus: boolean;
  latestReleasePlusMode: "fast" | "balanced" | "strict";
  lastVersion: string | null;
  lastLoader: string;
  windowW?: number;
  windowH?: number;
}

export interface UiSettings {
  locale: string;
}

export interface JvmTuningModules {
  lowLatencyGc: boolean;
  aggressiveJit: boolean;
  codeCache: boolean;
  g1Client: boolean;
}

export interface AdvancedSettings {
  enabled: boolean;
  debugVisible: boolean;
  jvmTuningMode: "smooth" | "performance_lab";
  jvmTuningModules: JvmTuningModules;
  jvmFlagsOverride: string;
  jdkOverride: string;
  maxConcurrentDownloads: number;
  keepGameOpen: boolean;
  keepLauncherVisible: boolean;
  logInspectorEnabled: boolean;
}

export interface LauncherSettings {
  schemaVersion: number;
  game: GameSettings;
  ui: UiSettings;
  advanced: AdvancedSettings;
  accounts: StoredAuth[];
  activeAccountUuid: string | null;
  shortcut?: string;
}

export interface AutoMod {
  project_id: string;
  name: string;
  description: string;
  icon_url?: string | null;
  enabled: boolean;
  tags: string[];
  /** empty = all loaders */
  loaders: string[];
  install_rank: number;
  keep_priority: number;
  min_mc_version?: string | null;
  max_mc_version?: string | null;
}

export interface ModFile {
  filename: string;
  size_bytes: number;
  display_name: string | null;
  icon_url: string | null;
}

export interface ModSearchResult {
  project_id: string;
  title: string;
  description: string;
  downloads: number;
  icon_url?: string;
  slug: string;
}

export interface RecommendedMod {
  project_id: string;
  name: string;
  description: string;
  icon_url?: string;
  /** "nvidia-only" | "beta" | "server-focus" */
  tags: string[];
  default_enabled: boolean;
  install_rank: number;
  keep_priority: number;
  min_mc_version?: string | null;
  max_mc_version?: string | null;
}

export interface ModpackVersionInfo {
  id: string;
  name: string;
  version_number: string;
  game_versions: string[];
}

export interface Profile {
  id: string;
  kind?: "normal" | "smart";
  name: string;
  mcVersion: string;
  loader: string;
  smartKey?: "latest-plus" | "snapshot-plus";
  channel?: "latest-release" | "latest-snapshot";
  loaderPolicy?: "fabric-then-vanilla";
  resolved?: {
    mcVersion: string;
    loader: string;
    loaderVersion?: string | null;
    resolvedAt: string;
  } | null;
  loaderVersion?: string;
  memoryMb?: number;
  windowW?: number;
  windowH?: number;
  lastLaunchedAt?: string;
  createdAt: string;
}
