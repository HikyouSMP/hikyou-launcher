import { useMemo } from "react";
import type {
  LoaderType,
  ModSearchResult,
  Profile,
  VersionEntry,
  VersionManifest,
} from "../types";
import type { parseIntent } from "../utils/intent";
import { fuzzyScore, vMatch, vScore } from "../utils/intent";
import { isLoaderCompatible } from "../utils/versionCompatibility";

type Intent = ReturnType<typeof parseIntent>;

const VERSION_RESULT_LIMIT = 12;

function semverDesc(a: string, b: string) {
  const pa = a.split(".").map(Number);
  const pb = b.split(".").map(Number);
  for (let i = 0; i < Math.max(pa.length, pb.length); i += 1) {
    const d = (pa[i] || 0) - (pb[i] || 0);
    if (d !== 0) return -d;
  }
  return 0;
}

function versionTimeMs(v: VersionEntry) {
  return Date.parse(v.releaseTime ?? v.time ?? "") || 0;
}

function newestFirst(a: VersionEntry, b: VersionEntry) {
  const byTime = versionTimeMs(b) - versionTimeMs(a);
  return byTime !== 0 ? byTime : semverDesc(a.id, b.id);
}

function snapshotPriority(v: VersionEntry) {
  return v.type === "snapshot" ? 0 : 1;
}

function snapshotBoost(v: VersionEntry, showSnapshots: boolean, intent: Intent) {
  if (v.type !== "snapshot") return 0;
  return showSnapshots || intent.isSnap ? 10_000 : 0;
}

function recencyScore(v: VersionEntry) {
  return versionTimeMs(v) / 10_000_000_000;
}

export function useProfileSearchModel({
  creating,
  intent,
  manifest,
  matchedProfiles,
  modpackResults,
  profiles,
  searchMode,
  showSnapshots,
}: {
  creating: boolean;
  intent: Intent;
  manifest: VersionManifest | null;
  matchedProfiles: Profile[];
  modpackResults: ModSearchResult[];
  profiles: Profile[];
  searchMode: "profile" | "modpack";
  showSnapshots: boolean;
}) {
  const candidateVersions = useMemo(() => {
    if (!manifest) return [];
    const all = manifest.versions.filter((v) => {
      if (intent.isSnap) return v.type === "snapshot";
      return v.type === "release" || (showSnapshots && v.type === "snapshot");
    }).sort((a, b) => {
      if (showSnapshots || intent.isSnap) {
        const byType = snapshotPriority(a) - snapshotPriority(b);
        if (byType !== 0) return byType;
      }
      return newestFirst(a, b);
    });

    const loaderFilter = (v: { id: string }) =>
      intent.loaderHint ? isLoaderCompatible(intent.loaderHint, v.id) : true;

    if (intent.empty) {
      return profiles.length === 0 ? all.slice(0, VERSION_RESULT_LIMIT) : [];
    }
    if (intent.isLatest || intent.isSnap) {
      return all.filter(loaderFilter).slice(0, VERSION_RESULT_LIMIT);
    }

    if (intent.verHint) {
      return all
        .filter((v) => vMatch(v.id, intent.verHint!))
        .filter(loaderFilter)
        .map((v) => ({
          v,
          score:
            vScore(v.id, intent.verHint!) * 1000 +
            snapshotBoost(v, showSnapshots, intent) +
            recencyScore(v),
        }))
        .sort((a, b) =>
          b.score !== a.score ? b.score - a.score : newestFirst(a.v, b.v),
        )
        .map((x) => x.v)
        .slice(0, VERSION_RESULT_LIMIT);
    }

    if (intent.nameTokens.length > 0) {
      const query = intent.nameTokens.join(" ");
      return all
        .map((v) => ({
          v,
          score:
            fuzzyScore(v.id, query) +
            snapshotBoost(v, showSnapshots, intent) +
            recencyScore(v),
        }))
        .filter((x) => x.score >= 0)
        .filter((x) => loaderFilter(x.v))
        .sort((a, b) => b.score - a.score || newestFirst(a.v, b.v))
        .map((x) => x.v)
        .slice(0, VERSION_RESULT_LIMIT);
    }

    if (intent.loaderHint) return all.filter(loaderFilter).slice(0, 10);

    return [];
  }, [manifest, intent, showSnapshots, profiles]);

  const loadersForCreate = useMemo((): LoaderType[] => {
    if (intent.loaderHint) return [intent.loaderHint];
    return ["vanilla", "fabric", "quilt", "forge", "neoforge"];
  }, [intent.loaderHint]);

  const navItems = useMemo(() => {
    if (creating) return [];
    if (searchMode === "modpack") {
      return modpackResults.map((_, i) => `modpack:${i}`);
    }

    const items: string[] = [];
    matchedProfiles.forEach((p) => items.push(`p:${p.id}`));
    const loaders: LoaderType[] = intent.loaderHint
      ? [intent.loaderHint]
      : ["vanilla", "fabric", "quilt", "forge", "neoforge"];

    candidateVersions.forEach((v) => {
      loaders.forEach((loader) => {
        if (isLoaderCompatible(loader, v.id)) {
          items.push(`c:${v.id}:${loader}`);
        }
      });
    });
    return items;
  }, [
    candidateVersions,
    creating,
    intent.loaderHint,
    matchedProfiles,
    modpackResults,
    searchMode,
  ]);

  return { candidateVersions, loadersForCreate, navItems };
}
