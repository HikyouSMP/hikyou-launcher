import { invoke } from "@tauri-apps/api/core";
import { ArrowLeft, Lock, Trash2, Upload } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import type { DatapackItem, WorldSummary } from "../../types";
import { formatBytes, readFileBytes } from "../../utils/fileImport";
import { EmptyLibraryState } from "./LibraryShell";

export function WorldLibraryPanel({ profileId }: { profileId: string }) {
  const { t } = useTranslation();
  const [worlds, setWorlds] = useState<WorldSummary[]>([]);
  const [selected, setSelected] = useState<WorldSummary | null>(null);
  const [datapacks, setDatapacks] = useState<DatapackItem[]>([]);
  const [error, setError] = useState<string | null>(null);
  const worldInput = useRef<HTMLInputElement>(null);
  const datapackInput = useRef<HTMLInputElement>(null);

  const loadWorlds = useCallback(async () => {
    try {
      const result = await invoke<WorldSummary[]>("list_worlds", { profileId });
      setWorlds(result);
      setError(null);
    } catch (reason) {
      setError(String(reason));
    }
  }, [profileId]);

  const loadDatapacks = useCallback(async () => {
    if (!selected) return;
    try {
      setDatapacks(await invoke("list_datapacks", { profileId, worldId: selected.id }));
      setError(null);
    } catch (reason) {
      setError(String(reason));
    }
  }, [profileId, selected]);

  useEffect(() => void loadWorlds(), [loadWorlds]);
  useEffect(() => void loadDatapacks(), [loadDatapacks]);

  const importWorld = async (file: File) => {
    try {
      await invoke("import_world", {
        profileId,
        filename: file.name,
        bytes: await readFileBytes(file, 512 * 1024 * 1024),
      });
      await loadWorlds();
    } catch (reason) {
      setError(String(reason));
    }
  };

  const importDatapack = async (file: File) => {
    if (!selected) return;
    try {
      await invoke("import_datapack", {
        profileId,
        worldId: selected.id,
        filename: file.name,
        bytes: await readFileBytes(file, 256 * 1024 * 1024),
      });
      await loadDatapacks();
    } catch (reason) {
      setError(String(reason));
    }
  };

  if (selected) {
    return (
      <div className="p-3">
        <div className="library-surface-heading">
          <div className="flex min-w-0 items-center gap-2">
          <button className="icon-btn" onClick={() => setSelected(null)} title={t("common.back")}>
            <ArrowLeft size={14} />
          </button>
          <div className="min-w-0 flex-1">
            <div className="truncate text-[13px] font-medium text-t1">{selected.name}</div>
            <div className="text-[10px] text-t3">
              {[selected.versionName, selected.gameMode, selected.hardcore ? "Hardcore" : null].filter(Boolean).join(" · ")}
            </div>
          </div>
          </div>
          <button
            className="icon-btn"
            disabled={selected.locked}
            onClick={() => datapackInput.current?.click()}
            title={t("library.import_datapack")}
          >
            <Upload size={14} />
          </button>
          <input
            ref={datapackInput}
            type="file"
            accept=".zip,application/zip"
            className="hidden"
            onChange={(event) => {
              const file = event.currentTarget.files?.[0];
              event.currentTarget.value = "";
              if (file) void importDatapack(file);
            }}
          />
        </div>
        {selected.locked && (
          <div className="mb-2 flex items-center gap-1.5 rounded-md bg-warning-bg px-2.5 py-2 text-[11px] text-warning">
            <Lock size={12} /> {t("library.world_in_use")}
          </div>
        )}
        {error && <div className="mb-2 rounded-md bg-danger-bg px-2.5 py-2 text-[11px] text-danger">{error}</div>}
        <div className="mb-2 px-1 text-[11px] text-t2">{t("library.datapacks")}</div>
        {datapacks.length === 0 ? (
          <EmptyLibraryState>{t("library.no_datapacks")}</EmptyLibraryState>
        ) : (
          <div className="space-y-1">
            {datapacks.map((item) => (
              <div className="library-row" key={item.name}>
                <div className="min-w-0 flex-1">
                  <div className="truncate text-[12px] text-t1">{item.name}</div>
                  <div className="text-[10px] text-t3">{item.directory ? t("library.folder") : formatBytes(item.sizeBytes)}</div>
                </div>
                <button
                  className="icon-btn danger"
                  disabled={selected.locked}
                  title={t("common.delete")}
                  onClick={async () => {
                    try {
                      await invoke("remove_datapack", { profileId, worldId: selected.id, name: item.name });
                      await loadDatapacks();
                    } catch (reason) {
                      setError(String(reason));
                    }
                  }}
                >
                  <Trash2 size={13} />
                </button>
              </div>
            ))}
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="p-3">
      <div className="library-surface-heading">
        <div>
          <div className="text-[14px] font-medium text-t1">{t("library.worlds")}</div>
          <div className="text-[10px] text-t3">{t("library.item_count", { count: worlds.length })}</div>
        </div>
        <button className="icon-btn" onClick={() => worldInput.current?.click()} title={t("library.import_world")}>
          <Upload size={14} />
        </button>
        <input
          ref={worldInput}
          type="file"
          accept=".zip,application/zip"
          className="hidden"
          onChange={(event) => {
            const file = event.currentTarget.files?.[0];
            event.currentTarget.value = "";
            if (file) void importWorld(file);
          }}
        />
      </div>
      {error && <div className="mb-2 rounded-md bg-danger-bg px-2.5 py-2 text-[11px] text-danger">{error}</div>}
      {worlds.length === 0 ? (
        <EmptyLibraryState>{t("library.no_worlds")}</EmptyLibraryState>
      ) : (
        <div className="space-y-1">
          {worlds.map((world) => (
            <button key={world.id} className="library-row w-full text-left" onClick={() => setSelected(world)}>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5">
                  <span className="truncate text-[12px] text-t1">{world.name}</span>
                  {world.locked && <Lock size={10} className="shrink-0 text-warning" />}
                </div>
                <div className="truncate text-[10px] text-t3">
                  {[world.versionName, world.gameMode, world.lastPlayed ? new Date(world.lastPlayed).toLocaleDateString() : null]
                    .filter(Boolean)
                    .join(" · ")}
                </div>
              </div>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
