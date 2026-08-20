import type { Dispatch, SetStateAction } from "react";
import { ArrowLeft } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useTranslation } from "react-i18next";

import i18n from "../i18n";
import { C } from "../theme";
import type { LauncherSettings } from "../types";
import { LogDropdown } from "./LogDropdown";
import { MaterialSlider } from "./MaterialSlider";
import { NumberInput } from "./NumberInput";
import { ShortcutCapture } from "./ShortcutCapture";
import { Pill, SBtn, SGroup, SRow, Toggle } from "./ui";
import { nestedViewEnterClass, type NavDirection } from "../utils/viewTransitions";

export type UpdateSettings = (
  patch: (settings: LauncherSettings) => LauncherSettings,
) => void;

export function SettingsView({
  settings,
  showSnapshots,
  setShowSnapshots,
  updateSettings,
  manifestLoading,
  defaultShortcut,
  isMacOS,
  navDirection,
  onBack,
  onRefreshManifest,
  onOpenRecommendedMods,
}: {
  settings: LauncherSettings;
  showSnapshots: boolean;
  setShowSnapshots: Dispatch<SetStateAction<boolean>>;
  updateSettings: UpdateSettings;
  manifestLoading: boolean;
  defaultShortcut: string;
  isMacOS: boolean;
  navDirection: NavDirection;
  onBack: () => void;
  onRefreshManifest: () => void;
  onOpenRecommendedMods: () => void;
}) {
  const { t } = useTranslation();
  const labModules = [
    {
      key: "lowLatencyGc",
      label: t("settings.jvm_lab_low_latency_gc"),
    },
    {
      key: "aggressiveJit",
      label: t("settings.jvm_lab_aggressive_jit"),
    },
    {
      key: "codeCache",
      label: t("settings.jvm_lab_code_cache"),
    },
    {
      key: "g1Client",
      label: t("settings.jvm_lab_g1_client"),
    },
  ] as const;

  return (
    <div
      className={
        nestedViewEnterClass(navDirection) +
        " sb flex-1 flex flex-col overflow-y-auto"
      }
    >
      <div
        data-tauri-drag-region
        className="flex items-center gap-2 px-4 h-14 shrink-0 border-b border-b1"
      >
        <Pill onClick={onBack} title={t("common.back")}>
          <ArrowLeft size={14} />
        </Pill>
        <span className="text-[15px] font-normal text-t1 tracking-[-0.02em]">
          {t("settings.title")}
        </span>
      </div>

      <div className="sb flex-1 overflow-y-auto p-4 flex flex-col gap-5">
        <SGroup label={t("settings.game_group")}>
          <SRow
            label={t("settings.memory_label")}
            sub={t("settings.memory_sub", { mb: settings.game.memoryMb })}
          >
            <div className="flex items-center gap-2">
              <span className="text-[12px] text-t1 font-mono font-normal min-w-13 text-right">
                {settings.game.memoryMb >= 1024
                  ? `${(settings.game.memoryMb / 1024).toFixed(1)}G`
                  : `${settings.game.memoryMb}M`}
              </span>
              <MaterialSlider
                min={512}
                max={16384}
                step={512}
                value={settings.game.memoryMb}
                width={86}
                onChange={(value) =>
                  updateSettings((state) => ({
                    ...state,
                    game: { ...state.game, memoryMb: value },
                  }))
                }
              />
            </div>
          </SRow>
          <SRow
            label={t("settings.snapshots_label")}
            sub={t("settings.snapshots_sub")}
          >
            <Toggle
              on={showSnapshots}
              onToggle={() => {
                const next = !showSnapshots;
                setShowSnapshots(next);
                updateSettings((state) => ({
                  ...state,
                  game: { ...state.game, showSnapshots: next },
                }));
              }}
            />
          </SRow>
          <SRow
            label={t("settings.launch_after_create_label")}
            sub={t("settings.launch_after_create_sub")}
          >
            <Toggle
              on={settings.game.launchAfterCreate !== false}
              onToggle={() =>
                updateSettings((state) => ({
                  ...state,
                  game: {
                    ...state.game,
                    launchAfterCreate: !(state.game.launchAfterCreate !== false),
                  },
                }))
              }
            />
          </SRow>
          <SRow
            label={t("settings.latest_plus_label")}
            sub={t("settings.latest_plus_sub")}
          >
            <Toggle
              on={settings.game.latestReleasePlus !== false}
              onToggle={() =>
                updateSettings((state) => ({
                  ...state,
                  game: {
                    ...state.game,
                    latestReleasePlus: !(state.game.latestReleasePlus !== false),
                  },
                }))
              }
            />
          </SRow>
          <SRow
            label={t("settings.snapshot_plus_label")}
            sub={t("settings.snapshot_plus_sub")}
          >
            <Toggle
              on={settings.game.snapshotPlus === true}
              onToggle={() =>
                updateSettings((state) => ({
                  ...state,
                  game: {
                    ...state.game,
                    snapshotPlus: state.game.snapshotPlus !== true,
                  },
                }))
              }
            />
          </SRow>
          <SRow
            label={t("settings.latest_plus_mode_label")}
            sub={t("settings.latest_plus_mode_sub")}
          >
            <div className="settings-dropdown-wrap">
              <LogDropdown
                value={settings.game.latestReleasePlusMode ?? "balanced"}
                options={[
                  { value: "fast", label: t("settings.latest_plus_fast") },
                  { value: "balanced", label: t("settings.latest_plus_balanced") },
                  { value: "strict", label: t("settings.latest_plus_strict") },
                ]}
                onChange={(mode) =>
                  updateSettings((state) => ({
                    ...state,
                    game: {
                      ...state.game,
                      latestReleasePlusMode: mode as "fast" | "balanced" | "strict",
                    },
                  }))
                }
              />
            </div>
          </SRow>
          <SRow
            label={t("settings.window_size_label")}
            sub={t("settings.window_size_sub")}
          >
            <div className="flex items-center gap-1">
              <NumberInput
                min={640}
                max={3840}
                step={8}
                value={settings.game.windowW ?? 854}
                onCommit={(value) =>
                  updateSettings((state) => ({
                    ...state,
                    game: { ...state.game, windowW: value },
                  }))
                }
                className="glass-input w-15 py-1 px-2 rounded-md text-[12px] text-t1 font-mono outline-none text-center"
              />
              <span className="text-t3 text-[11px]">×</span>
              <NumberInput
                min={480}
                max={2160}
                step={8}
                value={settings.game.windowH ?? 480}
                onCommit={(value) =>
                  updateSettings((state) => ({
                    ...state,
                    game: { ...state.game, windowH: value },
                  }))
                }
                className="glass-input w-15 py-1 px-2 rounded-md text-[12px] text-t1 font-mono outline-none text-center"
              />
            </div>
          </SRow>
          <SRow
            label={t("settings.refresh_version_label")}
            sub={t("settings.refresh_version_sub")}
            last
          >
            <SBtn onClick={onRefreshManifest} disabled={manifestLoading}>
              {manifestLoading ? t("common.updating") : t("common.update")}
            </SBtn>
          </SRow>
        </SGroup>

        <SGroup label={t("settings.rec_mods_group")}>
          <SRow
            label={t("settings.rec_mods_label")}
            sub={t("settings.rec_mods_sub")}
            last
          >
            <SBtn onClick={onOpenRecommendedMods}>
              {t("settings.rec_mods_btn")}
            </SBtn>
          </SRow>
        </SGroup>

        <SGroup label={t("settings.language_label")}>
          <SRow
            label={t("settings.language_label")}
            sub={t("settings.language_sub")}
            last
          >
            <div className="settings-dropdown-wrap">
              <LogDropdown
                value={i18n.language?.startsWith("ja") ? "ja" : "en"}
                options={[
                  { value: "en", label: "English" },
                  { value: "ja", label: "日本語" },
                ]}
                onChange={(lang) => {
                  i18n.changeLanguage(lang).catch(() => {});
                  WebviewWindow.getByLabel("game-log")
                    .then((window) => window?.emit("log://language-changed", lang))
                    .catch(() => {});
                  updateSettings((state) => ({
                    ...state,
                    ui: { ...state.ui, locale: lang },
                  }));
                }}
              />
            </div>
          </SRow>
        </SGroup>

        <SGroup label={t("settings.launcher_group")}>
          <SRow
            label={t("settings.keep_visible_label")}
            sub={t("settings.keep_visible_sub")}
          >
            <Toggle
              on={settings.advanced.keepLauncherVisible ?? false}
              onToggle={() =>
                updateSettings((state) => ({
                  ...state,
                  advanced: {
                    ...state.advanced,
                    keepLauncherVisible: !state.advanced.keepLauncherVisible,
                  },
                }))
              }
            />
          </SRow>
          <SRow
            label={t("settings.shortcut_label")}
            sub={t("settings.shortcut_sub")}
            last
          >
            <ShortcutCapture
              value={settings.shortcut ?? defaultShortcut}
              isMac={isMacOS}
              onConfirm={(shortcut) => {
                updateSettings((state) => ({ ...state, shortcut }));
                invoke("register_shortcut", { shortcutStr: shortcut }).catch(
                  console.error,
                );
              }}
            />
          </SRow>
        </SGroup>

        {settings.advanced?.enabled === true && (
          <SGroup label={t("settings.advanced_group")}>
            <SRow
              label={t("settings.jvm_tuning_label")}
              sub={t("settings.jvm_tuning_sub")}
            >
              <div className="settings-dropdown-wrap">
                <LogDropdown
                  value={settings.advanced.jvmTuningMode ?? "smooth"}
                  options={[
                    {
                      value: "smooth",
                      label: t("settings.jvm_tuning_smooth"),
                    },
                    {
                      value: "performance_lab",
                      label: t("settings.jvm_tuning_lab"),
                    },
                  ]}
                  onChange={(mode) =>
                    updateSettings((state) => ({
                      ...state,
                      advanced: {
                        ...state.advanced,
                        jvmTuningMode: mode as "smooth" | "performance_lab",
                      },
                    }))
                  }
                />
              </div>
            </SRow>
            {settings.advanced.jvmTuningMode === "performance_lab" && (
              <SRow
                label={t("settings.jvm_lab_modules_label")}
                sub={t("settings.jvm_lab_modules_sub")}
              >
                <div className="flex max-w-[310px] flex-wrap justify-end gap-1.5">
                  {labModules.map((module) => {
                    const enabled =
                      settings.advanced.jvmTuningModules?.[module.key] ?? true;
                    return (
                      <button
                        key={module.key}
                        type="button"
                        className={
                          enabled
                            ? "lab-module-chip is-on"
                            : "lab-module-chip"
                        }
                        onClick={() =>
                          updateSettings((state) => ({
                            ...state,
                            advanced: {
                              ...state.advanced,
                              jvmTuningModules: {
                                ...state.advanced.jvmTuningModules,
                                [module.key]:
                                  !state.advanced.jvmTuningModules[module.key],
                              },
                            },
                          }))
                        }
                      >
                        {module.label}
                      </button>
                    );
                  })}
                </div>
              </SRow>
            )}
            <SRow
              label={t("settings.jdk_override_label")}
              sub={t("settings.jdk_override_sub")}
            >
              <input
                type="text"
                value={settings.advanced.jdkOverride ?? ""}
                onChange={(event) =>
                  updateSettings((state) => ({
                    ...state,
                    advanced: {
                      ...state.advanced,
                      jdkOverride: event.target.value,
                    },
                  }))
                }
                placeholder="C:\\Program Files\\Java\\jdk-21"
                className="glass-input w-40 py-1.25 px-2 rounded-md text-[11px] text-t1 outline-none font-mono"
                onFocus={(event) => (event.target.style.borderColor = C.greenBdr)}
                onBlur={(event) => (event.target.style.borderColor = C.b1)}
              />
            </SRow>
            <SRow
              label={t("settings.jvm_flags_label")}
              sub={t("settings.jvm_flags_sub")}
            >
              <input
                type="text"
                value={settings.advanced.jvmFlagsOverride ?? ""}
                onChange={(event) =>
                  updateSettings((state) => ({
                    ...state,
                    advanced: {
                      ...state.advanced,
                      jvmFlagsOverride: event.target.value,
                    },
                  }))
                }
                placeholder="-XX:+UseZGC -Xmx4G"
                className="glass-input w-40 py-1.25 px-2 rounded-md text-[11px] text-t1 outline-none font-mono"
                onFocus={(event) => (event.target.style.borderColor = C.greenBdr)}
                onBlur={(event) => (event.target.style.borderColor = C.b1)}
              />
            </SRow>
            <SRow
              label={t("settings.max_downloads_label")}
              sub={t("settings.max_downloads_sub")}
            >
              <div className="flex items-center gap-1.5">
                <span className="text-[12px] text-t1 font-mono min-w-5 text-center">
                  {settings.advanced.maxConcurrentDownloads ?? 4}
                </span>
                <MaterialSlider
                  min={1}
                  max={16}
                  step={1}
                  value={settings.advanced.maxConcurrentDownloads ?? 4}
                  width={78}
                  onChange={(value) =>
                    updateSettings((state) => ({
                      ...state,
                      advanced: {
                        ...state.advanced,
                        maxConcurrentDownloads: value,
                      },
                    }))
                  }
                />
              </div>
            </SRow>
            <SRow
              label={t("settings.keep_open_label")}
              sub={t("settings.keep_open_sub")}
            >
              <Toggle
                on={settings.advanced.keepGameOpen ?? false}
                onToggle={() =>
                  updateSettings((state) => ({
                    ...state,
                    advanced: {
                      ...state.advanced,
                      keepGameOpen: !state.advanced.keepGameOpen,
                    },
                  }))
                }
              />
            </SRow>
            <SRow
              label={t("settings.debug_visible_label")}
              sub={t("settings.debug_visible_sub")}
            >
              <Toggle
                on={settings.advanced.debugVisible ?? false}
                onToggle={() =>
                  updateSettings((state) => ({
                    ...state,
                    advanced: {
                      ...state.advanced,
                      debugVisible: !state.advanced.debugVisible,
                    },
                  }))
                }
              />
            </SRow>
            <SRow
              label={t("settings.log_inspector_label")}
              sub={t("settings.log_inspector_sub")}
            >
              <Toggle
                on={settings.advanced.logInspectorEnabled ?? false}
                onToggle={() =>
                  updateSettings((state) => ({
                    ...state,
                    advanced: {
                      ...state.advanced,
                      logInspectorEnabled: !state.advanced.logInspectorEnabled,
                    },
                  }))
                }
              />
            </SRow>
            <SRow
              label={t("settings.disable_advanced_label")}
              sub={t("settings.disable_advanced_sub")}
              last
            >
              <button
                className="btn-ghost btn-phys px-3 py-1.5 rounded-md text-[12px] text-t2 cursor-pointer transition-all duration-100 bg-transparent border border-b1"
                onClick={() =>
                  updateSettings((state) => ({
                    ...state,
                    advanced: { ...state.advanced, enabled: false },
                  }))
                }
              >
                {t("common.disable")}
              </button>
            </SRow>
          </SGroup>
        )}
      </div>
    </div>
  );
}
