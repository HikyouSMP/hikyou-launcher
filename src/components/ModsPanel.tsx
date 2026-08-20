// ─────────────────────────────────────────────────────────────────────────────
// ModsPanel — Mod 管理
// ─────────────────────────────────────────────────────────────────────────────

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { C } from "../theme";
import type { ModFile, ModSearchResult } from "../types";
import { ModListView, type MLEntry } from "./ModListView";

interface Props {
  profileId: string;
  profileName: string;
  mcVersion: string;
  loader: string;
  onClose: () => void;
}

export function ModsPanel({ profileId, profileName: _profileName, mcVersion, loader, onClose }: Props) {
  const { t } = useTranslation();
  const [installed, setInstalled] = useState<ModFile[]>([]);
  const [loadingInstalled, setLoadingInstalled] = useState(true);
  const [removingFile, setRemovingFile] = useState<string | null>(null);
  const [installingId, setInstallingId] = useState<string | null>(null);
  const [installErr, setInstallErr] = useState<string | null>(null);
  const [justInstalled, setJustInstalled] = useState<Set<string>>(new Set());
  const justInstalledFilesRef = useRef<Map<string, string>>(new Map()); // filename -> project_id

  const ldrColor = () => { switch (loader) { case "fabric": return C.fabric; case "quilt": return C.quilt; case "forge": return C.forge; case "neoforge": return C.neoforge; default: return C.green; } };
  const ldrBg    = () => { switch (loader) { case "fabric": return C.fabricBg; case "quilt": return C.quiltBg; case "forge": return C.forgeBg; case "neoforge": return C.neoforgeBg; default: return C.greenBg; } };

  const findInstalledMod = (mod: ModSearchResult): ModFile | undefined => {
    return installed.find((m) => {
      const base = m.filename.replace(/\.disabled$/, "").replace(/\.jar$/, "");
      return (
        base.toLowerCase().includes(mod.slug.toLowerCase()) ||
        (m.display_name && m.display_name.toLowerCase() === mod.title.toLowerCase())
      );
    });
  };

  const loadInstalled = () => {
    setLoadingInstalled(true);
    invoke<ModFile[]>("get_profile_mods", { profileId })
      .then((mods) => { setInstalled(mods); })
      .catch(() => {})
      .finally(() => setLoadingInstalled(false));
  };

  useEffect(() => {
    setLoadingInstalled(true);
    invoke<ModFile[]>("get_profile_mods", { profileId })
      .then((mods) => {
        setInstalled(mods);
        setLoadingInstalled(false);
        invoke<ModFile[]>("backfill_mod_metadata", { profileId })
          .then((updated) => setInstalled(updated))
          .catch(() => {});
      })
      .catch(() => setLoadingInstalled(false));
  }, [profileId]); // eslint-disable-line

  const handleInstall = async (mod: ModSearchResult): Promise<void> => {
    setInstallingId(mod.project_id);
    setInstallErr(null);
    try {
      const files = await invoke<ModFile[]>("install_modrinth_mod", {
        profileId, projectId: mod.project_id, mcVersion, loader,
        displayName: mod.title, iconUrl: mod.icon_url ?? null,
      });
      setInstalled((prev) => {
        const names = new Set(prev.map((f) => f.filename));
        const newFiles = files.filter((f) => !names.has(f.filename));
        return [...prev, ...newFiles].sort((a, b) => (a.display_name || a.filename).localeCompare(b.display_name || b.filename));
      });
      setJustInstalled((prev) => new Set([...prev, mod.project_id]));
      files.forEach((f) => justInstalledFilesRef.current.set(f.filename, mod.project_id));
    } catch (e) {
      setInstallErr(String(e));
    } finally {
      setInstallingId(null);
    }
  };

  const handleToggle = async (filename: string): Promise<void> => {
    // Optimistic update: ファイル名を即座に反転してUIを先に更新する
    setInstalled((prev) => prev.map((m) => {
      if (m.filename !== filename) return m;
      const newFilename = m.filename.endsWith(".disabled")
        ? m.filename.slice(0, -".disabled".length)
        : `${m.filename}.disabled`;
      return { ...m, filename: newFilename };
    }));
    try {
      const updated = await invoke<ModFile>("toggle_profile_mod", { profileId, filename });
      // バックエンドの応答で確定（base名が一致する要素を置換）
      setInstalled((prev) => prev.map((m) => {
        const base = (f: string) => f.replace(/\.disabled$/, "");
        return base(m.filename) === base(updated.filename) ? updated : m;
      }));
    } catch {
      loadInstalled(); // エラー時はリロードで復元
    }
  };

  const handleRemove = async (filename: string): Promise<void> => {
    setRemovingFile(filename);
    try {
      await invoke("remove_profile_mod", { profileId, filename });
      setInstalled((prev) => prev.filter((m) => m.filename !== filename));
      const projectId = justInstalledFilesRef.current.get(filename);
      if (projectId) {
        justInstalledFilesRef.current.delete(filename);
        setJustInstalled((prev) => { const next = new Set(prev); next.delete(projectId); return next; });
      }
    } catch { /* ignore */ } finally {
      setRemovingFile(null);
    }
  };

  const entries: MLEntry[] = installed.map((m) => {
    const isDisabled = m.filename.endsWith(".disabled");
    const name = m.display_name || (isDisabled ? m.filename.replace(/\.disabled$/, "") : m.filename);
    return {
      id: m.filename,
      name,
      icon: m.icon_url,
      enabled: !isDisabled,
      subtitle: m.display_name ? m.filename : undefined,
    };
  });

  const searchStateFn = (projectId: string): "idle" | "loading" | "done" => {
    if (installingId === projectId) return "loading";
    if (justInstalled.has(projectId)) return "done";
    return "idle";
  };

  return (
    <ModListView
      onClose={onClose}
      searchPlaceholder={t("mods.search_placeholder")}
      headerRight={
        <>
          <span className="text-[10px] px-[6px] py-[1px] rounded-[6px] shrink-0 bg-surface text-t2" style={{ fontFamily: "'JetBrains Mono','SF Mono',monospace" }}>
            {mcVersion}
          </span>
          <span className="text-[10px] px-[6px] py-[1px] rounded-[6px] font-semibold tracking-[0.02em] shrink-0" style={{ background: ldrBg(), color: ldrColor() }}>
            {loader}
          </span>
          <button
            onClick={loadInstalled}
            title={t("mods.refresh_btn")}
            className="flex items-center px-[6px] py-1 rounded-[6px] cursor-pointer transition-colors duration-[120ms] bg-transparent border-0 text-t3"
            onMouseEnter={(e) => (e.currentTarget.style.color = C.t1)}
            onMouseLeave={(e) => (e.currentTarget.style.color = C.t3)}
          >
            <RefreshCw size={13} />
          </button>
        </>
      }
      entries={entries}
      loading={loadingInstalled || removingFile !== null}
      errorNode={installErr ? (
        <div className="mx-[10px] my-[6px] px-[10px] py-[6px] rounded-[6px] flex items-center justify-between gap-2 text-[11px] bg-danger-bg border border-danger-bdr text-danger">
          <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{installErr}</span>
          <button onClick={() => setInstallErr(null)} className="cursor-pointer text-sm leading-none shrink-0 bg-transparent border-0 text-danger">×</button>
        </div>
      ) : undefined}
      onEntryClick={(filename) => handleToggle(filename)}
      onEntryDelete={(filename) => handleRemove(filename)}
      loader={loader}
      mcVersion={mcVersion}
      searchState={searchStateFn}
      onInstall={handleInstall}
      onSearchResultDelete={(mod) => {
        const installedMod = findInstalledMod(mod);
        if (installedMod) handleRemove(installedMod.filename);
      }}
    />
  );
}
