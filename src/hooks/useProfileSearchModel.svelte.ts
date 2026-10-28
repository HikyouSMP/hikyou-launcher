import type { LoaderType, ModSearchResult, Profile, VersionEntry, VersionManifest, VersionChannel } from "../types";
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
        if (d !== 0)
            return -d;
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
function recencyScore(v: VersionEntry) {
    return versionTimeMs(v) / 10000000000;
}
export function useProfileSearchModel(params: {
    creating: boolean;
    intent: Intent;
    manifest: VersionManifest | null;
    matchedProfiles: Profile[];
    modpackResults: ModSearchResult[];
    profiles: Profile[];
    searchMode: "profile" | "modpack";
    versionChannel: VersionChannel;
}) {
    const candidateVersions = $derived.by(() => {
        if (!params.manifest)
            return [];
        if (params.intent.isSnap && params.versionChannel !== "snapshot")
            return [];
        const all = params.manifest.versions
            .filter(v => v.type === params.versionChannel)
            .sort(newestFirst);
        const loaderFilter = (v: {
            id: string;
        }) => params.intent.loaderHint ? isLoaderCompatible(params.intent.loaderHint, v.id) : true;
        if (params.intent.empty) {
            return params.profiles.length === 0 ? all.slice(0, VERSION_RESULT_LIMIT) : [];
        }
        if (params.intent.isLatest || params.intent.isSnap) {
            return all.filter(loaderFilter).slice(0, VERSION_RESULT_LIMIT);
        }
        if (params.intent.verHint) {
            return all
                .filter((v) => vMatch(v.id, params.intent.verHint!))
                .filter(loaderFilter)
                .map((v) => ({
                v,
                score: vScore(v.id, params.intent.verHint!) * 1000 + recencyScore(v),
            }))
                .sort((a, b) => b.score !== a.score ? b.score - a.score : newestFirst(a.v, b.v))
                .map((x) => x.v)
                .slice(0, VERSION_RESULT_LIMIT);
        }
        if (params.intent.nameTokens.length > 0) {
            const query = params.intent.nameTokens.join(" ");
            return all
                .map((v) => ({
                v,
                score: fuzzyScore(v.id, query),
            }))
                .filter((x) => x.score >= 0)
                .filter((x) => loaderFilter(x.v))
                .sort((a, b) => b.score - a.score || newestFirst(a.v, b.v))
                .map((x) => x.v)
                .slice(0, VERSION_RESULT_LIMIT);
        }
        if (params.intent.loaderHint)
            return all.filter(loaderFilter).slice(0, 10);
        return [];
    });
    const loadersForCreate = $derived.by((): LoaderType[] => {
        if (params.intent.loaderHint)
            return [params.intent.loaderHint];
        return ["vanilla", "fabric", "quilt", "forge", "neoforge"];
    });
    const navItems = $derived.by(() => {
        if (params.creating)
            return [];
        if (params.searchMode === "modpack") {
            return params.modpackResults.map((_, i) => `modpack:${i}`);
        }
        const items: string[] = [];
        params.matchedProfiles.forEach((p) => items.push(`p:${p.id}`));
        const loaders: LoaderType[] = params.intent.loaderHint
            ? [params.intent.loaderHint]
            : ["vanilla", "fabric", "quilt", "forge", "neoforge"];
        candidateVersions.forEach((v) => {
            loaders.forEach((loader) => {
                if (isLoaderCompatible(loader, v.id)) {
                    items.push(`c:${v.id}:${loader}`);
                }
            });
        });
        return items;
    });
    return {
        get candidateVersions() {
            return candidateVersions;
        },
        get loadersForCreate() {
            return loadersForCreate;
        },
        get navItems() {
            return navItems;
        }
    };
}
