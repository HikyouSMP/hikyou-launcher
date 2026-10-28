import type { AutoMod, LauncherSettings, Profile, VersionManifest } from "../../src/types";

export const profiles: Profile[] = [
  { id: "smoke-orchard", name: "Orchard Fabric", mcVersion: "1.21.1", loader: "fabric", loaderVersion: "0.16.10", createdAt: "2025-02-01T12:00:00Z" },
  { id: "smoke-alpine", name: "Alpine Vanilla", mcVersion: "1.20.1", loader: "vanilla", createdAt: "2025-01-01T12:00:00Z" },
  { id: "smoke-studio", name: "Studio Quilt", mcVersion: "1.21.1", loader: "quilt", loaderVersion: "0.28.0", createdAt: "2024-12-01T12:00:00Z" },
];

export const settings: LauncherSettings = {
  schemaVersion: 2,
  game: {
    memoryMb: 4096, versionChannel: "release", launchAfterCreate: false, latestReleasePlus: false,
    snapshotPlus: false, latestReleasePlusMode: "balanced", lastVersion: "1.21.1", lastLoader: "vanilla",
    windowW: 854, windowH: 480,
  },
  ui: { locale: "en" },
  advanced: {
    enabled: false, debugVisible: false, jvmTuningMode: "smooth",
    jvmTuningModules: { lowLatencyGc: false, aggressiveJit: false, codeCache: false, g1Client: false },
    jvmFlagsOverride: "", jdkOverride: "", maxConcurrentDownloads: 4,
    keepGameOpen: false, keepLauncherVisible: true, logInspectorEnabled: false,
  },
  accounts: [], activeAccountUuid: null, shortcut: "Alt+E",
};

export const manifest: VersionManifest = {
  latest: { release: "1.21.1", snapshot: "24w33a" },
  versions: [
    { id: "1.21.1", type: "release", url: "https://fixture.invalid/1.21.1.json", releaseTime: "2024-08-08T12:00:00Z" },
    { id: "1.20.1", type: "release", url: "https://fixture.invalid/1.20.1.json", releaseTime: "2023-06-12T12:00:00Z" },
    { id: "24w33a", type: "snapshot", url: "https://fixture.invalid/24w33a.json", releaseTime: "2024-08-15T12:00:00Z" },
  ],
};

export const autoMods: AutoMod[] = [{
  project_id: "smoke-sodium", name: "Sodium", description: "Fixture rendering mod", icon_url: null,
  enabled: true, tags: [], loaders: ["fabric"], install_rank: 1, keep_priority: 50,
}];

