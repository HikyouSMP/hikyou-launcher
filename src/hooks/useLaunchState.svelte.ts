import { untrack } from 'svelte';
import { resolveUpdate, type Setter } from '../lib/state';
import type { DownloadProgress, GamePhase } from "../types";
export function useLaunchState() {
    let profileRunStates = $state<Record<string, {
        phase: GamePhase;
        dlProgress: DownloadProgress | null;
    }>>({});
    const setProfileRunStates: Setter<Record<string, {
        phase: GamePhase;
        dlProgress: DownloadProgress | null;
    }>> = update => untrack(() => {
        profileRunStates = resolveUpdate(profileRunStates, update);
    });
    let gameError = $state<string | undefined>();
    const setGameError: Setter<(string | undefined) | undefined> = update => untrack(() => {
        gameError = resolveUpdate(gameError, update);
    });
    let profileLogs = $state<Record<string, string[]>>({});
    const setProfileLogs: Setter<Record<string, string[]>> = update => untrack(() => {
        profileLogs = resolveUpdate(profileLogs, update);
    });
    let profileCtxMenu = $state<{
        profileId: string;
        x: number;
        y: number;
    } | null>(null);
    const setProfileCtxMenu: Setter<{
        profileId: string;
        x: number;
        y: number;
    } | null> = update => untrack(() => {
        profileCtxMenu = resolveUpdate(profileCtxMenu, update);
    });
    return {
        get profileRunStates() {
            return profileRunStates;
        },
        get setProfileRunStates() {
            return setProfileRunStates;
        },
        get gameError() {
            return gameError;
        },
        get setGameError() {
            return setGameError;
        },
        get profileLogs() {
            return profileLogs;
        },
        get setProfileLogs() {
            return setProfileLogs;
        },
        get profileCtxMenu() {
            return profileCtxMenu;
        },
        get setProfileCtxMenu() {
            return setProfileCtxMenu;
        }
    };
}
