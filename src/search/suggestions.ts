import definitions from "./suggestions.json";
import type { VersionChannel } from "../types";
import { parseIntent } from "../utils/intent";

const SHORTCUTS = { toggleVersionChannel: "Tab", searchModpacks: "Space" } as const;
export type SearchSuggestionAction = keyof typeof SHORTCUTS;
export interface SearchSuggestion {
    id: string;
    action: SearchSuggestionAction;
    shortcut: typeof SHORTCUTS[SearchSuggestionAction];
    labelKey: string;
}

export function resolveSearchSuggestion(query: string, channel: VersionChannel): SearchSuggestion | null {
    const tokens = new Set(parseIntent(query).tokens);
    const normalized = query.trim().toLowerCase();
    for (const definition of definitions) {
        // Definitions select known commands, never executable code, URLs or IPC.
        if (definition.action !== "toggleVersionChannel" && definition.action !== "searchModpacks") continue;
        const shortcut = SHORTCUTS[definition.action];
        if (definition.shortcut !== shortcut) continue;
        const words = [definition.keyword, ...definition.aliases];
        if (!words.some(word => definition.match === "query" ? normalized === word : tokens.has(word))) continue;
        const labelKey = definition.labels?.[channel] ?? definition.label;
        if (!labelKey) continue;
        return {
            id: definition.id,
            action: definition.action,
            shortcut,
            labelKey,
        };
    }
    return null;
}
