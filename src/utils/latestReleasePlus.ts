import { invoke } from "@tauri-apps/api/core";

import type { LoaderVersion, Profile, VersionManifest } from "../types";

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
      mcVersion: manifest?.latest.release ?? profile.resolved?.mcVersion ?? "latest",
      loader: "auto",
    };
  }
  if (isSnapshotPlus(profile)) {
    return {
      ...profile,
      name: profile.name || "Snapshot+",
      mcVersion: manifest?.latest.snapshot ?? profile.resolved?.mcVersion ?? "latest",
      loader: "auto",
    };
  }
  return profile;
}

export async function resolveLatestReleasePlus(
  manifest: VersionManifest | null,
): Promise<Pick<Profile, "mcVersion" | "loader" | "loaderVersion">> {
  return resolveAutoLoader(manifest?.latest.release ?? "latest");
}

async function resolveAutoLoader(
  mcVersion: string,
): Promise<Pick<Profile, "mcVersion" | "loader" | "loaderVersion">> {
  try {
    const fabricVersions = await invoke<LoaderVersion[]>("get_fabric_versions", {
      mcVersion,
    });
    const stable = fabricVersions.find((version) => version.stable);
    const loaderVersion = stable?.version ?? fabricVersions[0]?.version ?? null;
    if (loaderVersion) {
      return { mcVersion, loader: "fabric", loaderVersion };
    }
  } catch {
    // Fabric can lag behind a brand-new Minecraft version. Vanilla fallback is
    // the intended behavior for dynamic "plus" profiles.
  }

  return { mcVersion, loader: "vanilla", loaderVersion: undefined };
}

export async function resolveSnapshotPlus(
  manifest: VersionManifest | null,
): Promise<Pick<Profile, "mcVersion" | "loader" | "loaderVersion">> {
  return resolveAutoLoader(manifest?.latest.snapshot ?? "latest");
}
