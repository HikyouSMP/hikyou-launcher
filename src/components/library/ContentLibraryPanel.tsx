import { invoke } from "@tauri-apps/api/core";
import { Trash2, Upload } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import type { ProfileContentItem } from "../../types";
import { formatBytes, readFileBytes } from "../../utils/fileImport";
import { EmptyLibraryState } from "./LibraryShell";

type ContentKind = "shader" | "resource_pack";

export function ContentLibraryPanel({
  profileId,
  kind,
}: {
  profileId: string;
  kind: ContentKind;
}) {
  const { t } = useTranslation();
  const [items, setItems] = useState<ProfileContentItem[]>([]);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const reload = useCallback(async () => {
    try {
      setItems(await invoke("list_profile_content", { profileId, kind }));
      setError(null);
    } catch (reason) {
      setError(String(reason));
    }
  }, [kind, profileId]);

  useEffect(() => void reload(), [reload]);

  const importFile = async (file: File) => {
    try {
      await invoke("import_profile_content", {
        profileId,
        kind,
        filename: file.name,
        bytes: await readFileBytes(file, 256 * 1024 * 1024),
      });
      await reload();
    } catch (reason) {
      setError(String(reason));
    }
  };

  return (
    <div className="p-3">
      <div className="library-surface-heading">
        <div>
          <div className="text-[14px] font-medium text-t1">{kind === "shader" ? t("library.shaders") : t("library.resource_packs")}</div>
          <div className="text-[10px] text-t3">{t("library.item_count", { count: items.length })}</div>
        </div>
        <div className="flex items-center gap-1.5">
          <button className="icon-btn" onClick={() => inputRef.current?.click()} title={t("library.import") }>
            <Upload size={14} />
          </button>
          <input
            ref={inputRef}
            type="file"
            accept=".zip,application/zip"
            className="hidden"
            onChange={(event) => {
              const file = event.currentTarget.files?.[0];
              event.currentTarget.value = "";
              if (file) void importFile(file);
            }}
          />
        </div>
      </div>
      {error && <div className="mb-2 rounded-md bg-danger-bg px-2.5 py-2 text-[11px] text-danger">{error}</div>}
      {items.length === 0 ? (
        <EmptyLibraryState>{t("library.no_content")}</EmptyLibraryState>
      ) : (
        <div className="space-y-1">
          {items.map((item) => (
            <div key={item.name} className="library-row">
              <div className="min-w-0 flex-1">
                <div className="truncate text-[12px] text-t1">{item.name}</div>
                <div className="text-[10px] text-t3">{item.directory ? t("library.folder") : formatBytes(item.sizeBytes)}</div>
              </div>
              <button
                className="icon-btn danger"
                title={t("common.delete")}
                onClick={async () => {
                  try {
                    await invoke("remove_profile_content", { profileId, kind, name: item.name });
                    await reload();
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
