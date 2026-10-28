import { untrack } from 'svelte';
import { type Setter } from '../../lib/state';
import { invoke } from "@tauri-apps/api/core";
import type { ModSearchResult } from "../../types";
type Args = {
    active: boolean;
    query: string;
    setModpackResults: Setter<ModSearchResult[]>;
    setModpackSearching: Setter<boolean>;
};
export function useModpackSearch(params: Args) {
    $effect(() => {
        void params.active;
        void params.query;
        void params.setModpackResults;
        void params.setModpackSearching;
        return untrack(() => {
            const trimmed = params.query.trim();
            if (!params.active || !trimmed) {
                params.setModpackResults([]);
                params.setModpackSearching(false);
                return;
            }
            let cancelled = false;
            params.setModpackSearching(true);
            const timer = setTimeout(() => {
                invoke<ModSearchResult[]>("search_modrinth_modpacks", {
                    query: trimmed,
                    mcVersion: "",
                })
                    .then(results => { if (!cancelled) params.setModpackResults(results); })
                    .catch(() => { })
                    .finally(() => { if (!cancelled) params.setModpackSearching(false); });
            }, 350);
            return () => {
                cancelled = true;
                clearTimeout(timer);
            };
        });
    });
}
