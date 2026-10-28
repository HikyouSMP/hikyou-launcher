import { untrack } from 'svelte';
import { resolveUpdate, type Setter } from '../lib/state';
import { invoke } from "@tauri-apps/api/core";
import type { LauncherSettings, StoredAuth } from "../types";
export function useAuth(updateSettings: (patch: (p: LauncherSettings) => LauncherSettings) => void) {
    let savedAuth = $state<StoredAuth | null>(null);
    let authLoaded = $state(false);
    let revision = 0;
    const setSavedAuth: Setter<StoredAuth | null> = update => untrack(() => {
        revision++;
        savedAuth = resolveUpdate(savedAuth, update);
        authLoaded = true;
    });
    const setAuthLoaded: Setter<boolean> = update => untrack(() => {
        authLoaded = resolveUpdate(authLoaded, update);
    });
    const loadSavedAuth = () => {
        const request = ++revision;
        setAuthLoaded(false);
        return invoke<StoredAuth>("get_saved_auth")
            .then((auth) => {
            if (request !== revision) return;
            savedAuth = auth;
            updateSettings((settings) => {
                const exists = settings.accounts.some((x) => x.uuid === auth.uuid);
                if (exists)
                    return settings;
                return {
                    ...settings,
                    accounts: [auth, ...settings.accounts],
                    activeAccountUuid: settings.activeAccountUuid ?? auth.uuid ?? null,
                };
            });
        })
            .catch(() => { if (request === revision) savedAuth = null; })
            .finally(() => { if (request === revision) setAuthLoaded(true); });
    };
    return {
        get savedAuth() {
            return savedAuth;
        },
        get setSavedAuth() {
            return setSavedAuth;
        },
        get authLoaded() {
            return authLoaded;
        },
        get loadSavedAuth() {
            return loadSavedAuth;
        }
    };
}
