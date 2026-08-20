// ─────────────────────────────────────────────────────────────────────────────
// RecModsPanel — 自動インストールMod設定 (auto_mods.json ベース)
// ─────────────────────────────────────────────────────────────────────────────

import { useState } from "react";
import { useTranslation } from "react-i18next";
import { C } from "../theme";
import type { AutoMod, ModSearchResult } from "../types";
import { ModListView, type MLEntry } from "./ModListView";

interface Props {
  autoMods: AutoMod[];
  onSave: (mods: AutoMod[]) => void;
  onClose: () => void;
}

export function RecModsPanel({
  autoMods, onSave, onClose,
}: Props) {
  const { t } = useTranslation();
  const [addingId, setAddingId] = useState<string | null>(null);
  const [justAdded, setJustAdded] = useState<Set<string>>(new Set());

  const toggleById = (id: string) => {
    onSave(autoMods.map(m => m.project_id === id ? { ...m, enabled: !m.enabled } : m));
  };

  const deleteById = (id: string) => {
    onSave(autoMods.filter(m => m.project_id !== id));
  };

  const addFromSearch = async (mod: ModSearchResult): Promise<void> => {
    if (autoMods.some(m => m.project_id === mod.project_id) || justAdded.has(mod.project_id)) return;
    setAddingId(mod.project_id);
    const newMod: AutoMod = {
      project_id: mod.project_id,
      name: mod.title,
      description: mod.description,
      icon_url: mod.icon_url ?? null,
      enabled: true,
      tags: [],
      loaders: [], // カスタムMod = 全ローダー対応
      install_rank: 2,
      keep_priority: 50,
    };
    onSave([...autoMods, newMod]);
    setJustAdded(prev => new Set([...prev, mod.project_id]));
    setAddingId(null);
  };

  const isAlreadyInList = (projectId: string): boolean =>
    autoMods.some(m => m.project_id === projectId);

  const searchStateFn = (projectId: string): "idle" | "loading" | "done" => {
    if (addingId === projectId) return "loading";
    if (justAdded.has(projectId) || isAlreadyInList(projectId)) return "done";
    return "idle";
  };

  const entries: MLEntry[] = autoMods.map(m => ({
    id: m.project_id,
    name: m.name,
    icon: m.icon_url,
    enabled: m.enabled,
    subtitle: m.description,
    badges: (
      <>
        {m.tags.includes("nvidia-only") && (
          <span className="shrink-0 text-[9px] px-[5px] py-[1px] rounded-[3px] leading-[1.6]" style={{ background: "rgba(184,144,48,.12)", color: C.warning }}>Nvidia</span>
        )}
        {m.tags.includes("beta") && (
          <span className="shrink-0 text-[9px] px-[5px] py-[1px] rounded-[3px] leading-[1.6]" style={{ background: C.dangerBg, color: C.danger }}>β</span>
        )}
        {m.tags.includes("opt-in") && (
          <span className="shrink-0 text-[9px] px-[5px] py-[1px] rounded-[3px] leading-[1.6]" style={{ background: C.hover, color: C.t3 }}>opt-in</span>
        )}
        {m.tags.includes("unsupported-gpu") && (
          <span className="shrink-0 text-[9px] px-[5px] py-[1px] rounded-[3px] leading-[1.6]" style={{ background: C.hover, color: C.t3 }}>GPU</span>
        )}
        {m.loaders.length === 0 && (
          <span className="shrink-0 text-[9px] px-[5px] py-[1px] rounded-[3px] leading-[1.6]" style={{ background: C.hover, color: C.t3 }}>{t("mods.custom_badge")}</span>
        )}
      </>
    ),
  }));

  return (
    <ModListView
      onClose={onClose}
      searchPlaceholder={t("mods.rec_search_placeholder")}
      entries={entries}
      loading={false}
      emptyNode={
        <div className="py-8 px-4 text-center text-xs text-t3 leading-[1.6]">
          {t("mods.rec_empty")}<br /><span className="text-[10px]">{t("mods.rec_empty_hint")}</span>
        </div>
      }
      onEntryClick={toggleById}
      onEntryDelete={deleteById}
      loader=""
      mcVersion=""
      searchState={searchStateFn}
      onInstall={addFromSearch}
    />
  );
}
