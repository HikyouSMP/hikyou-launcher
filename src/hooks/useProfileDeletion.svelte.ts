import { untrack } from 'svelte';
import { resolveUpdate, type Setter } from '../lib/state';
import { invoke } from "@tauri-apps/api/core";
import type { Profile } from "../types";
type Args = {
    profiles: Profile[];
    setProfiles: Setter<Profile[]>;
    activeProfileId: string | null;
    setActiveProfileId: Setter<string | null>;
    isProfileBusy: (profileId: string) => boolean;
    profileRunStates: unknown;
};
export function useProfileDeletion(params: Args) {
    let deleteConfirmId = $state<string | null>(null);
    let retired = $state<{ id: string; name: string } | null>(null);
    let error = $state<string | null>(null);
    let pending = $state(false);
    const setDeleteConfirmId: Setter<string | null> = update => untrack(() => {
        deleteConfirmId = resolveUpdate(deleteConfirmId, update);
        if (!deleteConfirmId) { retired = null; error = null; }
    });
    $effect(() => {
        void deleteConfirmId;
        void params.isProfileBusy;
        void params.profileRunStates;
        return untrack(() => {
            if (deleteConfirmId && params.isProfileBusy(deleteConfirmId)) {
                setDeleteConfirmId(null);
            }
        });
    });
    const handleDeleteProfile = (id: string) => {
        if (params.profiles.find((profile) => profile.id === id)?.kind === "smart")
            return;
        if (params.isProfileBusy(id))
            return;
        retired = null;
        error = null;
        setDeleteConfirmId(id);
    };
    const handleDeleteProfileConfirm = async () => {
        if (!deleteConfirmId || pending || retired)
            return;
        if (params.isProfileBusy(deleteConfirmId)) {
            setDeleteConfirmId(null);
            return;
        }
        const targetId = deleteConfirmId;
        const name = params.profiles.find(profile => profile.id === targetId)?.name ?? targetId;
        pending = true;
        error = null;
        try {
            await invoke("delete_profile", { id: targetId });
        } catch (error) {
            if (deleteConfirmId === targetId) setError(String(error));
            return;
        } finally {
            pending = false;
        }
        params.setProfiles((prev) => {
            const next = prev.filter((profile) => profile.id !== targetId);
            if (params.activeProfileId === targetId) {
                params.setActiveProfileId(next[0]?.id ?? null);
            }
            return next;
        });
        if (deleteConfirmId === targetId) retired = { id: targetId, name };
    };
    function setError(message: string) { error = message; }
    const handleRestore = async () => {
        if (!retired || pending) return;
        const target = retired.id;
        pending = true; error = null;
        try {
            const restored = await invoke<Profile>("restore_retired_profile", { id: target });
            params.setProfiles(previous => [restored, ...previous.filter(profile => profile.id !== restored.id)]);
            params.setActiveProfileId(restored.id);
            if (deleteConfirmId === target) setDeleteConfirmId(null);
        } catch (failure) {
            if (deleteConfirmId === target) error = String(failure);
        } finally { pending = false; }
    };
    return {
        get retired() { return retired !== null; },
        get pending() { return pending; },
        get error() { return error; },
        get handleRestore() { return handleRestore; },
        get deleteConfirmId() {
            return deleteConfirmId;
        },
        get setDeleteConfirmId() {
            return setDeleteConfirmId;
        },
        get handleDeleteProfile() {
            return handleDeleteProfile;
        },
        get handleDeleteProfileConfirm() {
            return handleDeleteProfileConfirm;
        },
        get deleteProfileName() {
            return retired?.name ?? params.profiles.find((profile) => profile.id === deleteConfirmId)?.name ?? null;
        }
    };
}
