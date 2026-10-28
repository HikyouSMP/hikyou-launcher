// Only imported by the Playwright route; never add this to the production entry.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit, listen } from "@tauri-apps/api/event";
import type { LauncherSettings, ModEntry, ModRevision } from "../../src/types";
import type { JavaInventory } from "../../src/javaRuntimes";
import type { IdentifiedModFile, ModRequirement, RequestedState } from "../../src/modRequests";
import { autoMods, manifest, profiles, settings as initialSettings } from "./data";

const label = "main";
let settings = structuredClone(initialSettings);
if (new URL(location.href).searchParams.has("smoke_debug")) {
  settings.advanced.enabled = true;
  settings.advanced.debugVisible = true;
}
if (new URL(location.href).searchParams.has("smoke_legacy_java")) {
  settings.advanced.enabled = true;
  settings.advanced.jdkOverride = "/smoke/legacy/bin/java";
}
const calls: Array<{ command: string; args: unknown }> = [];
const unexpected: string[] = [];
let javaInventory: (JavaInventory & Record<string, unknown>) | null = null;
let manualJavaChecks = 0;
const fixtureProfiles = structuredClone(profiles);
if (new URL(location.href).searchParams.has("smoke_history_smart")) {
  fixtureProfiles[0] = { ...fixtureProfiles[0], id: "smart:latest-plus", kind: "smart", smartKey: "latest-plus" };
  settings.game.latestReleasePlus = true;
}
const historyProfile = fixtureProfiles[0].id;
const oldMod: ModEntry = {
  filename: "sodium-0.5.11.jar.disabled", objectId: "a".repeat(64), sizeBytes: 1024, enabled: false,
  modId: "sodium", version: "0.5.11", projectId: "smoke-sodium", providerVersion: "provider-old", providerVersionNumber: "mc1.20.1-0.5.11", displayName: "Sodium",
};
const currentMod: ModEntry = {
  ...oldMod, filename: "sodium-0.6.0.jar", objectId: "b".repeat(64), enabled: true, version: "0.6.0", providerVersion: "provider-current", providerVersionNumber: "mc1.21.1-0.6.0",
};
let revisions: ModRevision[] = [
  { id: "1".repeat(64), profileId: historyProfile, parent: "2".repeat(64), createdAt: "2025-02-03T12:00:00Z",
    target: { mcVersion: "1.21.1", loader: "fabric", loaderVersion: "0.16.10" }, mods: [currentMod, {
      filename: "iris.jar", objectId: "c".repeat(64), sizeBytes: 20, enabled: true, modId: "iris", version: "1.8.0",
    }] },
  { id: "2".repeat(64), profileId: historyProfile, parent: null, createdAt: "2025-02-01T12:00:00Z",
    target: { mcVersion: "1.21.1", loader: "fabric", loaderVersion: "0.16.10" }, mods: [oldMod, {
      filename: "lithium.jar", objectId: "d".repeat(64), sizeBytes: 20, enabled: true, modId: "lithium", version: "0.12.7",
    }] },
];
let activeMods: ModEntry[] | null = null;
const requestScenario = new URL(location.href).searchParams.has("smoke_mod_requests");
let requestState: RequestedState | null = null;
let requestFiles: IdentifiedModFile[] | null = null;
let requestFailure: "get" | "add" | "remove" | "toggle" | "remove_file" | null = null;
let holdModRead = false;
let releaseModRead: (() => void) | null = null;
function modFiles(): IdentifiedModFile[] {
  return structuredClone(requestFiles ?? (activeMods ? activeMods.map((mod) => ({
    filename: mod.filename, display_name: mod.displayName ?? null, icon_url: null, size_bytes: mod.sizeBytes,
    version_number: mod.version, version_id: mod.providerVersion, project_id: mod.projectId ?? null,
  })) : [{ filename: "sodium-fixture.jar", display_name: "Sodium", icon_url: null, size_bytes: 1024, project_id: "smoke-sodium" }]));
}
function modRequests(profileId: string): RequestedState {
  return structuredClone(requestState ?? {
    intent: { schema: 1, profileId, excludedProjects: [], requirements: activeMods ? activeMods.map((mod) => ({
      projectId: mod.projectId ?? null, objectId: mod.objectId, filename: mod.filename, enabled: mod.enabled,
      displayName: mod.displayName ?? null, iconUrl: null, origin: "imported",
    })) : [{ projectId: "smoke-sodium", objectId: null, filename: "sodium-fixture.jar", enabled: true,
      displayName: "Sodium", iconUrl: null, origin: "user" }], },
    activeRevisionId: revisions[0].id, pending: false, preparation: null,
  });
}
function failRequest(operation: typeof requestFailure) {
  if (requestFailure === operation) { requestFailure = null; throw new Error(`Fixture Mod request ${operation} refused`); }
}
function queueRequest(profileId: string, change: (requirements: ModRequirement[]) => ModRequirement[]) {
  const state = modRequests(profileId);
  state.intent.requirements = change(state.intent.requirements);
  state.pending = true;
  state.preparation = { intentId: "fixture-intent", phase: "queued", reason: null, updatedAt: 1738540800 };
  requestState = state;
}
if (historyProfile.startsWith("smart:")) {
  revisions[1].target = { mcVersion: "1.20.1", loader: "fabric", loaderVersion: "0.15.11" };
}
let historyFailure: string | null = null;
let pinned = false;
function historyArgs(args: Record<string, unknown>, keys: string[]) {
  if (Object.keys(args).sort().join(",") !== keys.sort().join(",") || args.profileId !== historyProfile) {
    throw new Error("Invalid history command arguments");
  }
}

function cloneWire<T>(value: T): T {
  return JSON.parse(JSON.stringify(value));
}

const noops = new Set([
  "hide_main_window", "register_shortcut", "suspend_shortcut",
  "plugin:window|show", "plugin:window|hide", "plugin:window|set_focus",
  "plugin:window|set_size", "plugin:window|set_position", "plugin:window|center",
  "plugin:window|start_dragging", "plugin:window|set_always_on_top",
]);
const measurement = { status: "not_applicable", detail: "Isolated browser fixture", evidence: "No OS provider connected" };

mockWindows(label);
// isTauri() uses this public environment flag, independently of mockWindows().
Object.defineProperty(globalThis, "isTauri", { configurable: true, value: true });
mockIPC((command, payload) => {
  // Svelte state may contain proxies; native Tauri IPC serializes these to JSON.
  const args = cloneWire(payload ?? {}) as Record<string, unknown>;
  calls.push({ command, args: cloneWire(args) });
  if (noops.has(command)) return null;
  switch (command) {
    case "get_settings": return structuredClone(settings);
    case "save_settings": settings = cloneWire(args.settings as LauncherSettings); return null;
    case "list_profiles": return structuredClone(fixtureProfiles);
    case "get_version_manifest":
    case "refresh_version_manifest": return structuredClone(manifest);
    case "get_auto_mods":
    case "init_auto_mods": return structuredClone(autoMods);
    case "get_saved_auth": throw new Error("No saved account in the isolated smoke fixture");
    case "get_default_shortcut": return "Alt+E";
    case "detect_gpu_vendor": return "unknown";
    case "get_smart_profile_statuses":
    case "get_launch_metric_history": return [];
    case "get_secure_storage_diagnostics": return {
      backend: "smoke-memory", measuredAtUnixMs: Date.UTC(2025, 1, 3), provider: null,
      keyScope: measurement, hardwareBacking: measurement, exportPolicy: measurement,
      userPresence: measurement, pcrBinding: measurement, accessControl: measurement,
    };
    case "get_launcher_paths": return { profiles: "/smoke/virtual/profiles", smart_profiles: "/smoke/virtual/smart", java_versions: "/smoke/virtual/java" };
    case "pick_java_runtime": return Number(args.major) === 8 ? null : {
      path: `/smoke/custom-${args.major}/bin/java`, major: args.major, javaVersion: `${args.major}.0.1`, vendor: "Fixture JRE",
    };
    case "validate_java_runtime": {
      if (String(args.path).includes("wrong")) throw new Error(`Expected Java ${args.major}, but this runtime is Java 8`);
      return new Promise((resolve) => setTimeout(() => resolve({ path: args.path, major: args.major, javaVersion: `${args.major}.0.1`, vendor: "Fixture JRE" }), 100));
    }
    case "get_auth_token_debug_status": return {
      minecraft_access: { available: false, persisted: false, managed_by_os: false },
      microsoft_refresh: { available: false, persisted: false, managed_by_os: false },
    };
    case "check_java_updates": {
      if (Object.keys(args).length) throw new Error("Manual update check takes no runtime paths");
      manualJavaChecks++;
      return new Promise((resolve, reject) => setTimeout(() => {
        if (manualJavaChecks > 1) { reject(new Error("Fixture manual update failure")); return; }
        const status = structuredClone(javaInventory ?? { installations: [], checks: [] });
        for (const runtime of status.installations) {
          if (runtime.major === 25 && runtime.selected) runtime.javaVersion = "25.0.5+8-LTS";
        }
        status.checks = status.checks.map(check => ({ ...check, checking: false, lastError: null, deferred: false }));
        javaInventory = status;
        resolve({ status, checked: 2, failures: [] });
      }, 350));
    }
    case "get_java_debug_info": return javaInventory ??= {
      found: true, java_version: 25, java_full_version: "25.0.4+1-LTS", java_dist: "Azul Zulu",
      java_path: "/smoke/virtual/java/zulu-25/bin/java", use_zgc: false, memory_mb: args.memoryMb,
      installations: [
        { directory: "zulu-25-current", executable: "/smoke/virtual/java/zulu-25-current/bin/java", major: 25, javaVersion: "25.0.4+1-LTS", vendor: "Azul Systems, Inc.", selected: true, legacy: false, error: null },
        { directory: "zulu-25-retained", executable: "/smoke/virtual/java/zulu-25-retained/bin/java", major: 25, javaVersion: "25.0.3+7-LTS", vendor: "Azul Systems, Inc.", selected: false, legacy: false, error: null },
        { directory: "zulu-8-current", executable: "/smoke/virtual/java/zulu-8-current/bin/java", major: 8, javaVersion: "1.8.0_504-b08", vendor: "Azul Systems, Inc.", selected: true, legacy: false, error: null },
        { directory: "liberica-nik-21", executable: "/smoke/virtual/java/liberica-nik-21/bin/java", major: 21, javaVersion: "21.0.8+9-LTS", vendor: "BellSoft", selected: false, legacy: true, error: null },
        { directory: "zulu-8-broken", executable: "/smoke/virtual/java/zulu-8-broken/bin/java", major: 8, javaVersion: "Unknown", vendor: "Unknown", selected: false, legacy: false, error: "Fixture missing Java binary" },
      ],
      checks: [
        { major: 25, checking: false, deferred: true, cleanupError: null, lastCheckedAt: 1738540800, lastAttemptAt: 1738540800, nextCheckAt: 1738627200, lastError: null },
        { major: 21, checking: false, deferred: false, cleanupError: "Fixture cleanup failure; will retry", lastCheckedAt: null, lastAttemptAt: 1738540800, nextCheckAt: 1738544400, lastError: "Fixture network failure; previous runtime retained" },
        { major: 17, checking: false, deferred: false, cleanupError: null, lastCheckedAt: null, lastAttemptAt: null, nextCheckAt: null, lastError: null },
        { major: 8, checking: true, deferred: false, cleanupError: null, lastCheckedAt: null, lastAttemptAt: 1738540800, nextCheckAt: null, lastError: null },
      ],
    };
    case "get_fabric_versions":
    case "get_quilt_versions":
    case "get_neoforge_versions": return [{ version: "0.16.10", stable: true }];
    case "get_profile_mods":
    case "backfill_mod_metadata": return modFiles();
    case "get_profile_mod_requests": {
      if (Object.keys(args).join() !== "profileId") throw new Error("Invalid Mod request read arguments");
      failRequest("get");
      const state = modRequests(String(args.profileId));
      if (holdModRead) {
        holdModRead = false;
        return new Promise((resolve) => { releaseModRead = () => { releaseModRead = null; resolve(state); }; });
      }
      return state;
    }
    case "install_modrinth_mod": {
      failRequest("add");
      queueRequest(String(args.profileId), (requirements) => [...requirements.filter((item) => item.projectId !== args.projectId), {
        projectId: String(args.projectId), objectId: null, filename: null, enabled: true,
        displayName: String(args.displayName), iconUrl: null, origin: "user",
      }]);
      return modFiles();
    }
    case "remove_requested_mod": {
      if (Object.keys(args).sort().join() !== "profileId,requestedKey") throw new Error("Invalid Mod request removal arguments");
      failRequest("remove");
      queueRequest(String(args.profileId), (requirements) => requirements.filter((item) =>
        (item.projectId ? `modrinth:${item.projectId}` : `local:${item.objectId}`) !== args.requestedKey));
      return null;
    }
    case "toggle_profile_mod": {
      if (!requestScenario) throw new Error("Fixture toggle refused");
      failRequest("toggle");
      const file = modFiles().find((item) => item.filename === args.filename);
      if (!file) throw new Error("Unknown fixture Mod file");
      queueRequest(String(args.profileId), (requirements) => requirements.map((item) => item.projectId === file.project_id ? { ...item, enabled: !item.enabled } : item));
      return file;
    }
    case "remove_profile_mod": {
      if (!requestScenario) throw new Error("Fixture remove refused");
      failRequest("remove_file");
      const file = modFiles().find((item) => item.filename === args.filename);
      queueRequest(String(args.profileId), (requirements) => requirements.filter((item) => item.projectId !== file?.project_id));
      return null;
    }
    case "get_profile_mod_history": {
      historyArgs(args, ["profileId"]);
      if (historyFailure === "history") { historyFailure = null; throw new Error("Fixture history unavailable"); }
      return structuredClone(revisions);
    }
    case "get_profile_mod_revision_diff": {
      historyArgs(args, ["profileId", "fromId", "toId"]);
      if (historyFailure === "diff") { historyFailure = null; throw new Error("Fixture comparison unavailable"); }
      const from = revisions.find((revision) => revision.id === args.fromId);
      const to = revisions.find((revision) => revision.id === args.toId);
      if (!from || !to) throw new Error("Unknown fixture revision");
      const key = (mod: ModEntry) => mod.projectId || mod.modId || mod.filename;
      return structuredClone({ fromId: from.id, toId: to.id, fromTarget: from.target, toTarget: to.target,
        added: to.mods.filter((mod) => !from.mods.some((other) => key(mod) === key(other))),
        removed: from.mods.filter((mod) => !to.mods.some((other) => key(mod) === key(other))),
        changed: to.mods.flatMap((after) => {
          const before = from.mods.find((mod) => key(mod) === key(after));
          return before && JSON.stringify(before) !== JSON.stringify(after) ? [{ before, after }] : [];
        }),
      });
    }
    case "restore_profile_mod_revision":
    case "resume_profile_mod_updates": {
      const resume = command === "resume_profile_mod_updates";
      historyArgs(args, resume ? ["profileId"] : ["profileId", "revisionId"]);
      if (historyFailure === "restore") { historyFailure = null; throw new Error("Fixture restore refused"); }
      if (resume && !historyProfile.startsWith("smart:")) throw new Error("Smart profile required");
      const selected = resume ? revisions.find((revision) => revision.id === "1".repeat(64)) : revisions.find((revision) => revision.id === args.revisionId);
      if (!selected) throw new Error("Unknown fixture revision");
      const restored = { ...selected, id: String(revisions.length + 1).repeat(64), parent: revisions[0].id, createdAt: "2025-02-04T12:00:00Z" };
      revisions = [restored, ...revisions]; activeMods = restored.mods; pinned = !resume && historyProfile.startsWith("smart:");
      return structuredClone(restored);
    }
    case "list_profile_content": return [{ name: `${args.kind}-fixture.zip`, sizeBytes: 4096, directory: false }];
    case "list_worlds": return [{
      id: "smoke-world", name: "Orchard World", lastPlayed: Date.UTC(2025, 1, 3), gameMode: "Survival",
      hardcore: false, versionName: "1.21.1", dataVersion: 3955, locked: false, iconAvailable: false,
    }];
    case "list_datapacks": return [{ name: "orchard-recipes.zip", sizeBytes: 1536, directory: false }];
    case "list_servers": return [{ key: "smoke-server", name: "Fixture Server", address: "server.invalid", hasIcon: false, acceptsTextures: null }];
    case "search_modrinth": return requestScenario ? [
      { project_id: "smoke-pending", title: "Sodium", description: "Different project with the same name", downloads: 123, slug: "sodium" },
      { project_id: "smoke-sodium", title: "Renamed project", description: "Same project with a different name", downloads: 456, slug: "different-name" },
    ] : [{ project_id: "smoke-sodium", title: "Sodium", description: "Fixture mod", downloads: 12345, slug: "sodium" }];
    case "search_modrinth_modpacks": return [];
    case "plugin:app|version": return "26.1.0";
    case "plugin:window|get_all_windows": return [label];
    case "plugin:window|is_focused":
    case "plugin:window|is_visible": return true;
    case "plugin:window|is_maximized": return false;
    case "plugin:window|scale_factor": return 1;
    case "plugin:window|inner_size":
    case "plugin:window|outer_size": return { width: innerWidth, height: innerHeight };
    case "plugin:window|inner_position":
    case "plugin:window|outer_position": return { x: 0, y: 0 };
    default:
      unexpected.push(command);
      throw new Error(`Unmocked or forbidden smoke IPC: ${command}`);
  }
}, { shouldMockEvents: true });

Object.defineProperty(window, "__HIKYOU_SMOKE__", { value: {
  label, calls, unexpected,
  snapshot: () => ({ label, calls: cloneWire(calls), unexpected: [...unexpected], settings: cloneWire(settings), pinned,
    modRequests: modRequests(historyProfile), modFiles: modFiles(), modReadHeld: releaseModRead !== null }),
  failHistoryOnce: (operation: "history" | "diff" | "restore") => { historyFailure = operation; },
  failModRequestOnce: (operation: NonNullable<typeof requestFailure>) => { requestFailure = operation; },
  setModRequests: (state: RequestedState, files?: IdentifiedModFile[]) => {
    requestState = cloneWire(state);
    if (files) requestFiles = cloneWire(files);
  },
  emitModEvent: (name: "mods://prepared" | "mods://requests-changed", profileId = historyProfile) => emit(name, { profileId, resolved: {} }),
  holdNextModRequestRead: () => { holdModRead = true; },
  releaseModRequestRead: () => releaseModRead?.(),
  async checkEvents() {
    let received = false;
    const stop = await listen<{ value: number }>("smoke://event-roundtrip", (event) => { received = event.payload.value === 42; });
    await emit("smoke://event-roundtrip", { value: 42 });
    stop(); return received;
  },
} });
