import { useCallback, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
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
    if (!a.lastLaunchedAt && !b.lastLaunchedAt) return 0;
    if (!a.lastLaunchedAt) return 1;
    if (!b.lastLaunchedAt) return -1;
    return b.lastLaunchedAt.localeCompare(a.lastLaunchedAt);
  });
}

export function useProfiles() {
  const [profiles, setProfiles] = useState<Profile[]>([]);

  const refreshProfiles = useCallback(async () => {
    const loaded = await invoke<Profile[]>("list_profiles");
    const sorted = sortProfiles(loaded);
    setProfiles(sorted);
    return sorted;
  }, []);

  return { profiles, setProfiles, refreshProfiles };
}
