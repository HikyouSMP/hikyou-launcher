import { untrack } from 'svelte';
import { resolveUpdate, type Setter } from '../lib/state';
import { invoke } from "@tauri-apps/api/core";
import type { ModSearchResult, ModpackVersionInfo } from "../types";
export function useModpackVersionDialog() {
    const pendingProjects = new Set<string>();
    let versionDialogModpack = $state<ModSearchResult | null>(null);
    const setVersionDialogModpack: Setter<ModSearchResult | null> = update => untrack(() => {
        versionDialogModpack = resolveUpdate(versionDialogModpack, update);
    });
    let modpackVersionsCache = $state<Record<string, ModpackVersionInfo[]>>({});
    const setModpackVersionsCache: Setter<Record<string, ModpackVersionInfo[]>> = update => untrack(() => {
        modpackVersionsCache = resolveUpdate(modpackVersionsCache, update);
    });
    let loadingVersionsFor = $state<string | null>(null);
    const setLoadingVersionsFor: Setter<string | null> = update => untrack(() => {
        loadingVersionsFor = resolveUpdate(loadingVersionsFor, update);
    });
    let modpackVersionIdx = $state(0);
    const setModpackVersionIdx: Setter<typeof modpackVersionIdx> = update => untrack(() => {
        modpackVersionIdx = resolveUpdate(modpackVersionIdx, update);
    });
    let hoverModpackVersionIdx = $state<number | null>(null);
    const setHoverModpackVersionIdx: Setter<number | null> = update => untrack(() => {
        hoverModpackVersionIdx = resolveUpdate(hoverModpackVersionIdx, update);
    });
    const openModpackVersionDialog = (modpack: ModSearchResult) => {
        setVersionDialogModpack(modpack);
        setModpackVersionIdx(0);
        setHoverModpackVersionIdx(null);
        if (!modpackVersionsCache[modpack.project_id] &&
            !pendingProjects.has(modpack.project_id)) {
            pendingProjects.add(modpack.project_id);
            setLoadingVersionsFor(modpack.project_id);
            invoke<ModpackVersionInfo[]>("get_modpack_versions", {
                projectId: modpack.project_id,
            })
                .then((versions) => setModpackVersionsCache((prev) => ({
                ...prev,
                [modpack.project_id]: versions,
            })))
                .catch(console.error)
                .finally(() => {
                    pendingProjects.delete(modpack.project_id);
                    if (loadingVersionsFor === modpack.project_id) setLoadingVersionsFor(null);
                });
        }
    };
    const closeModpackVersionDialog = () => {
        setVersionDialogModpack(null);
        setHoverModpackVersionIdx(null);
    };
    return {
        get versionDialogModpack() {
            return versionDialogModpack;
        },
        get setVersionDialogModpack() {
            return setVersionDialogModpack;
        },
        get modpackVersionsCache() {
            return modpackVersionsCache;
        },
        get loadingVersionsFor() {
            return loadingVersionsFor;
        },
        get modpackVersionIdx() {
            return modpackVersionIdx;
        },
        get setModpackVersionIdx() {
            return setModpackVersionIdx;
        },
        get hoverModpackVersionIdx() {
            return hoverModpackVersionIdx;
        },
        get setHoverModpackVersionIdx() {
            return setHoverModpackVersionIdx;
        },
        get openModpackVersionDialog() {
            return openModpackVersionDialog;
        },
        get closeModpackVersionDialog() {
            return closeModpackVersionDialog;
        }
    };
}
