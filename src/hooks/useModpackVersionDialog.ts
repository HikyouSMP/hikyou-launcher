import { useCallback, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ModSearchResult, ModpackVersionInfo } from "../types";

export function useModpackVersionDialog() {
  const [versionDialogModpack, setVersionDialogModpack] =
    useState<ModSearchResult | null>(null);
  const [modpackVersionsCache, setModpackVersionsCache] = useState<
    Record<string, ModpackVersionInfo[]>
  >({});
  const [loadingVersionsFor, setLoadingVersionsFor] = useState<string | null>(
    null,
  );
  const [modpackVersionIdx, setModpackVersionIdx] = useState(0);
  const [hoverModpackVersionIdx, setHoverModpackVersionIdx] = useState<
    number | null
  >(null);

  const openModpackVersionDialog = useCallback(
    (modpack: ModSearchResult) => {
      setVersionDialogModpack(modpack);
      setModpackVersionIdx(0);
      setHoverModpackVersionIdx(null);
      if (
        !modpackVersionsCache[modpack.project_id] &&
        loadingVersionsFor !== modpack.project_id
      ) {
        setLoadingVersionsFor(modpack.project_id);
        invoke<ModpackVersionInfo[]>("get_modpack_versions", {
          projectId: modpack.project_id,
        })
          .then((versions) =>
            setModpackVersionsCache((prev) => ({
              ...prev,
              [modpack.project_id]: versions,
            })),
          )
          .catch(console.error)
          .finally(() => setLoadingVersionsFor(null));
      }
    },
    [loadingVersionsFor, modpackVersionsCache],
  );

  const closeModpackVersionDialog = useCallback(() => {
    setVersionDialogModpack(null);
    setHoverModpackVersionIdx(null);
  }, []);

  return {
    versionDialogModpack,
    setVersionDialogModpack,
    modpackVersionsCache,
    loadingVersionsFor,
    modpackVersionIdx,
    setModpackVersionIdx,
    hoverModpackVersionIdx,
    setHoverModpackVersionIdx,
    openModpackVersionDialog,
    closeModpackVersionDialog,
  };
}
