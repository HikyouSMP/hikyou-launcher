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
    const setDeleteConfirmId: Setter<string | null> = update => untrack(() => {
        deleteConfirmId = resolveUpdate(deleteConfirmId, update);
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
        setDeleteConfirmId(id);
    };
    const handleDeleteProfileConfirm = async () => {
        if (!deleteConfirmId)
            return;
        if (params.isProfileBusy(deleteConfirmId)) {
            setDeleteConfirmId(null);
            return;
        }
        const targetId = deleteConfirmId;
        try {
            await invoke("delete_profile", { id: targetId });
        } catch (error) {
            console.error(error);
            return;
        }
        params.setProfiles((prev) => {
            const next = prev.filter((profile) => profile.id !== targetId);
            if (params.activeProfileId === targetId) {
                params.setActiveProfileId(next[0]?.id ?? null);
            }
            return next;
        });
        if (deleteConfirmId === targetId) setDeleteConfirmId(null);
    };
    return {
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
            return params.profiles.find((profile) => profile.id === deleteConfirmId)?.name ?? null;
        }
    };
}
