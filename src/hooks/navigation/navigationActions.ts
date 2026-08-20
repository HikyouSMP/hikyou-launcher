import { invoke } from "@tauri-apps/api/core";
import type { ModSearchResult } from "../../types";

export function searchModpacksNow(
  query: string,
  setModpackResults: React.Dispatch<React.SetStateAction<ModSearchResult[]>>,
  setModpackSearching: React.Dispatch<React.SetStateAction<boolean>>,
) {
  const trimmed = query.trim();
  if (!trimmed) return;
  setModpackSearching(true);
  invoke<ModSearchResult[]>("search_modrinth_modpacks", {
    query: trimmed,
    mcVersion: "",
  })
    .then(setModpackResults)
    .catch(() => {})
    .finally(() => setModpackSearching(false));
}
