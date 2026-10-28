import type { Profile, VersionManifest } from "../types";

export const LATEST_RELEASE_PLUS_ID = "smart:latest-plus";
export const SNAPSHOT_PLUS_ID = "smart:snapshot-plus";

export function isLatestReleasePlus(profile?: Profile | null) {
  return profile?.id === LATEST_RELEASE_PLUS_ID || profile?.smartKey === "latest-plus";
}

export function isSnapshotPlus(profile?: Profile | null) {
  return profile?.id === SNAPSHOT_PLUS_ID || profile?.smartKey === "snapshot-plus";
}

export function isSmartProfile(profile?: Profile | null) {
  return isLatestReleasePlus(profile) || isSnapshotPlus(profile);
}

export function withSmartProfileDisplay(
  profile: Profile,
  manifest: VersionManifest | null,
): Profile {
  if (isLatestReleasePlus(profile)) {
    return {
      ...profile,
      name: profile.name || "Latest+",
      mcVersion: profile.resolved?.mcVersion ?? manifest?.latest.release ?? "latest",
      loader: "auto",
    };
  }
  if (isSnapshotPlus(profile)) {
    return {
      ...profile,
      name: profile.name || "Snapshot+",
      mcVersion: profile.resolved?.mcVersion ?? manifest?.latest.snapshot ?? "latest",
      loader: "auto",
    };
  }
  return profile;
}
