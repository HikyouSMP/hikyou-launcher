import { untrack } from 'svelte';
import { resolveUpdate, type Setter } from '../lib/state';
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Profile } from "../types";
export function sortProfiles(profiles: Profile[]) {
    return [...profiles].sort((a, b) => {
        const aSmart = a.kind === "smart";
        const bSmart = b.kind === "smart";
        if (aSmart || bSmart) {
            if (aSmart && bSmart) {
                const order = ["smart:latest-plus", "smart:snapshot-plus"];
                return order.indexOf(a.id) - order.indexOf(b.id);
            }
            return aSmart ? -1 : 1;
        }
        if (!a.lastLaunchedAt && !b.lastLaunchedAt)
            return 0;
        if (!a.lastLaunchedAt)
            return 1;
        if (!b.lastLaunchedAt)
            return -1;
        return b.lastLaunchedAt.localeCompare(a.lastLaunchedAt);
    });
}
export function useProfiles() {
    let profiles = $state<Profile[]>([]);
    const setProfiles: Setter<Profile[]> = update => untrack(() => {
        profiles = resolveUpdate(profiles, update);
    });
    const refreshProfiles = async () => {
        const loaded = await invoke<Profile[]>("list_profiles");
        const sorted = sortProfiles(loaded);
        setProfiles(sorted);
        return sorted;
    };
    $effect(() => {
        let disposed = false;
        let unlisten: (() => void) | undefined;
        void listen<{ profileId: string; resolved: NonNullable<Profile["resolved"]> }>("mods://prepared", ({ payload }) => {
            if (!disposed) setProfiles(previous => previous.map(profile => profile.id === payload.profileId ? { ...profile, resolved: payload.resolved } : profile));
        }).then(stop => { if (disposed) stop(); else unlisten = stop; }).catch(error => console.error("mod preparation events:", error));
        return () => { disposed = true; unlisten?.(); };
    });
    return {
        get profiles() {
            return profiles;
        },
        get setProfiles() {
            return setProfiles;
        },
        get refreshProfiles() {
            return refreshProfiles;
        }
    };
}
