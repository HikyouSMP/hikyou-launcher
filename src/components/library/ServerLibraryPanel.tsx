import { invoke } from "@tauri-apps/api/core";
import { Check, Plus, Trash2, X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import type { ServerSummary } from "../../types";
import { EmptyLibraryState } from "./LibraryShell";

export function ServerLibraryPanel({ profileId }: { profileId: string }) {
  const { t } = useTranslation();
  const [servers, setServers] = useState<ServerSummary[]>([]);
  const [editing, setEditing] = useState<ServerSummary | "new" | null>(null);
  const [name, setName] = useState("");
  const [address, setAddress] = useState("");
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      setServers(await invoke("list_servers", { profileId }));
      setError(null);
    } catch (reason) {
      setError(String(reason));
    }
  }, [profileId]);

  useEffect(() => void reload(), [reload]);

  const beginEdit = (server: ServerSummary | "new") => {
    setEditing(server);
    setName(server === "new" ? "" : server.name);
    setAddress(server === "new" ? "" : server.address);
  };

  return (
    <div className="p-3">
      <div className="library-surface-heading">
        <div>
          <div className="text-[14px] font-medium text-t1">{t("library.servers")}</div>
          <div className="text-[10px] text-t3">{t("library.item_count", { count: servers.length })}</div>
        </div>
        <button className="icon-btn" onClick={() => beginEdit("new")} title={t("library.add_server")}>
          <Plus size={14} />
        </button>
      </div>
      {error && <div className="mb-2 rounded-md bg-danger-bg px-2.5 py-2 text-[11px] text-danger">{error}</div>}
      {editing && (
        <form
          className="mb-2 grid grid-cols-[1fr_1.25fr_auto] gap-1.5 rounded-md bg-surface-raised p-2"
          onSubmit={async (event) => {
            event.preventDefault();
            try {
              await invoke("save_server", {
                profileId,
                key: editing === "new" ? null : editing.key,
                name,
                address,
              });
              setEditing(null);
              await reload();
            } catch (reason) {
              setError(String(reason));
            }
          }}
        >
          <input className="library-input" value={name} onChange={(event) => setName(event.target.value)} placeholder={t("library.server_name")} autoFocus />
          <input className="library-input" value={address} onChange={(event) => setAddress(event.target.value)} placeholder={t("library.server_address")} />
          <div className="flex gap-1">
            <button className="icon-btn" type="submit" title={t("common.save")}><Check size={13} /></button>
            <button className="icon-btn" type="button" onClick={() => setEditing(null)} title={t("common.close")}><X size={13} /></button>
          </div>
        </form>
      )}
      {servers.length === 0 ? (
        <EmptyLibraryState>{t("library.no_servers")}</EmptyLibraryState>
      ) : (
        <div className="space-y-1">
          {servers.map((server) => (
            <div className="library-row" key={server.key}>
              <button className="min-w-0 flex-1 text-left" onClick={() => beginEdit(server)}>
                <div className="truncate text-[12px] text-t1">{server.name}</div>
                <div className="truncate text-[10px] text-t3">{server.address}</div>
              </button>
              <button
                className="icon-btn danger"
                title={t("common.delete")}
                onClick={async () => {
                  try {
                    await invoke("remove_server", { profileId, key: server.key });
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
