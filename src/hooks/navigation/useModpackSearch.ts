import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ModSearchResult } from "../../types";

type Args = {
  active: boolean;
  query: string;
  setModpackResults: React.Dispatch<React.SetStateAction<ModSearchResult[]>>;
  setModpackSearching: React.Dispatch<React.SetStateAction<boolean>>;
};

export function useModpackSearch({
  active,
  query,
  setModpackResults,
  setModpackSearching,
}: Args) {
  useEffect(() => {
    if (!active) return;
    const trimmed = query.trim();
    if (!trimmed) {
      setModpackResults([]);
      return;
    }
    setModpackSearching(true);
    const timer = setTimeout(() => {
      invoke<ModSearchResult[]>("search_modrinth_modpacks", {
        query: trimmed,
        mcVersion: "",
      })
        .then(setModpackResults)
        .catch(() => {})
        .finally(() => setModpackSearching(false));
    }, 350);
    return () => clearTimeout(timer);
  }, [active, query, setModpackResults, setModpackSearching]);
}
