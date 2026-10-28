import type { Profile } from "../types";
import { fuzzyScore, parseIntent, vMatch } from "../utils/intent";
import { isLatestReleasePlus, isSnapshotPlus } from "../utils/latestReleasePlus";
type ParsedIntent = ReturnType<typeof parseIntent>;
export function useMatchedProfiles(profiles: Profile[], intent: ParsedIntent, latestVersion: string) {
    return (() => {
        const matched: Array<{
            profile: Profile;
            score: number;
        }> = [];
        for (const profile of profiles) {
            if (intent.empty) {
                matched.push({ profile, score: 0 });
                continue;
            }
            if (intent.isLatest && profile.mcVersion !== latestVersion && !isLatestReleasePlus(profile))
                continue;
            if (intent.isSnap &&
                !isSnapshotPlus(profile) && !/(snapshot|-pre\d+|-rc\d+|^\d{2}w\d{2}[a-z])/i.test(profile.mcVersion)) {
                continue;
            }
            if (intent.loaderHint && profile.loader !== intent.loaderHint)
                continue;
            if (intent.verHint && !vMatch(profile.mcVersion, intent.verHint)) {
                continue;
            }
            const query = intent.nameTokens.join(" ");
            if (!query) {
                matched.push({ profile, score: 0 });
                continue;
            }
            const score = Math.max(fuzzyScore(profile.name, query), fuzzyScore(profile.mcVersion, query), fuzzyScore(profile.loader, query), fuzzyScore(`${profile.name} ${profile.loader} ${profile.mcVersion}`, query));
            if (score >= 0)
                matched.push({ profile, score });
        }
        return matched.sort((a, b) => b.score - a.score).map((x) => x.profile);
    })();
}
