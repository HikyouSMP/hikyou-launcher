import type { LoaderType } from "../types";

export function semverGte(
  versionId: string,
  major: number,
  minor: number,
  patch = 0,
): boolean {
  const parts = versionId.split(".").map(Number);
  const [mj = 0, mn = 0, pt = 0] = parts;
  if (mj !== major) return mj > major;
  if (mn !== minor) return mn > minor;
  return pt >= patch;
}

export function isNeoForgeCompatible(versionId: string): boolean {
  return semverGte(versionId, 1, 20, 2);
}

export function isFabricCompatible(versionId: string): boolean {
  return semverGte(versionId, 1, 14);
}

export function isForgeCompatible(versionId: string): boolean {
  return semverGte(versionId, 1, 1);
}

export function isLoaderCompatible(
  loader: LoaderType,
  versionId: string,
): boolean {
  if (loader === "neoforge") return isNeoForgeCompatible(versionId);
  if (loader === "forge") return isForgeCompatible(versionId);
  if (loader === "fabric" || loader === "quilt") {
    return isFabricCompatible(versionId);
  }
  return true;
}
