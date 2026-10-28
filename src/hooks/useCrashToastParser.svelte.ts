import { untrack } from 'svelte';
import { type Setter } from '../lib/state';
import { invoke } from "@tauri-apps/api/core";
import { createTranslation } from '../lib/i18n.svelte';
import type { CrashAnalysis, ParsedCrash } from "../types";
type Args = {
    crashToastProfileId: string | null;
    crashAnalyses: Record<string, CrashAnalysis>;
    setCrashAnalyses: Setter<Record<string, CrashAnalysis>>;
};
export function useCrashToastParser(params: Args) {
    const translation = createTranslation();
    const parsedSources = new Map<string, { lines: string[]; language: string }>();
    $effect(() => {
        const profileId = params.crashToastProfileId;
        const toast = profileId ? params.crashAnalyses[profileId] : null;
        const language = translation.language?.startsWith('en') ? 'en' : 'ja';
        if (!toast?.lines.length) return;
        const lines = toast.lines;
        const previous = parsedSources.get(toast.profile_id);
        // Writing the parsed result must not parse the same input again.
        if (previous?.lines === lines && previous.language === language) return;
        return untrack(() => {
            let cancelled = false;
            invoke<ParsedCrash>("parse_crash_log", {
                logLines: lines,
                lang: language,
            })
                .then((parsed) => {
                if (cancelled) return;
                parsedSources.set(toast.profile_id, { lines, language });
                params.setCrashAnalyses(prev => {
                    const current = prev[toast.profile_id];
                    if (current?.lines !== lines) return prev;
                    return { ...prev, [toast.profile_id]: { ...current, parsed } };
                });
            })
                .catch(error => {
                    if (!cancelled) {
                        parsedSources.delete(toast.profile_id);
                        console.error(error);
                    }
                });
            return () => { cancelled = true; };
        });
    });
}
