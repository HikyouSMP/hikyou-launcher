import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Profile } from "../types";

type Args = {
  profiles: Profile[];
  setProfiles: React.Dispatch<React.SetStateAction<Profile[]>>;
  activeProfileId: string | null;
  setActiveProfileId: React.Dispatch<React.SetStateAction<string | null>>;
  isProfileBusy: (profileId: string) => boolean;
  profileRunStates: unknown;
};

export function useProfileDeletion({
  profiles,
  setProfiles,
  activeProfileId,
  setActiveProfileId,
  isProfileBusy,
  profileRunStates,
}: Args) {
  const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null);

  useEffect(() => {
    if (deleteConfirmId && isProfileBusy(deleteConfirmId)) {
      setDeleteConfirmId(null);
    }
  }, [deleteConfirmId, isProfileBusy, profileRunStates]);

  const handleDeleteProfile = useCallback(
    (id: string) => {
      if (profiles.find((profile) => profile.id === id)?.kind === "smart") return;
      if (isProfileBusy(id)) return;
      setDeleteConfirmId(id);
    },
    [isProfileBusy, profiles],
  );

  const handleDeleteProfileConfirm = useCallback(async () => {
    if (!deleteConfirmId) return;
    if (isProfileBusy(deleteConfirmId)) {
      setDeleteConfirmId(null);
      return;
    }
    await invoke("delete_profile", { id: deleteConfirmId }).catch(
      console.error,
    );
    setProfiles((prev) => {
      const next = prev.filter((profile) => profile.id !== deleteConfirmId);
      if (activeProfileId === deleteConfirmId) {
        setActiveProfileId(next[0]?.id ?? null);
      }
      return next;
    });
    setDeleteConfirmId(null);
  }, [
    activeProfileId,
    deleteConfirmId,
    isProfileBusy,
    setActiveProfileId,
    setProfiles,
  ]);

  return {
    deleteConfirmId,
    setDeleteConfirmId,
    handleDeleteProfile,
    handleDeleteProfileConfirm,
    deleteProfileName:
      profiles.find((profile) => profile.id === deleteConfirmId)?.name ?? null,
  };
}
