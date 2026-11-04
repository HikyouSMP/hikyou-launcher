import type { DecisionView, ModChoice, ModChoiceRow, ModDecisionAction, ModLifecycleTarget, ModSnapshot } from "../../src/modLifecycle";
import type { IdentifiedModFile, ModRequirement, RequestedState } from "../../src/modRequests";

type FixtureFile = IdentifiedModFile & { objectId?: string };
type Options = {
  profileId: string;
  files: () => FixtureFile[];
  requested: () => RequestedState;
  save: (state: RequestedState) => void;
  target: () => ModLifecycleTarget;
  recommendations?: { projectId: string; name: string; enabled: boolean }[];
};

// Synthetic state only: no file, provider, account, launch or world operations.
export function createModLifecycleFixture(options: Options) {
  let revision = 0;
  let proposals: DecisionView[] = [];
  const names = new Map<string, string>();
  const keyOf = (item: { projectId: string | null; objectId: string | null }) => item.projectId ? `modrinth:${item.projectId}` : item.objectId ? `local:${item.objectId}` : null;
  const clone = <T>(value: T): T => structuredClone(value);
  function snapshot(): ModSnapshot {
    const state = options.requested();
    const requirements = new Map(state.intent.requirements.flatMap(item => { const key = keyOf(item); return key ? [[key, item] as const] : []; }));
    const files = new Map(options.files().map(file => {
      const key = keyOf({ projectId: file.project_id ?? null, objectId: file.objectId ?? null });
      if (!key) throw new Error("Fixture files require explicit provider or object identity");
      return [key, file] as const;
    }));
    const recommendations = new Map((options.recommendations ?? []).map(item => [`modrinth:${item.projectId}`, item]));
    const exclusions = new Set(state.intent.excludedProjects.map(id => `modrinth:${id}`));
    const keys = new Set([...requirements.keys(), ...files.keys(), ...exclusions, ...recommendations.keys()]);
    const rows: ModChoiceRow[] = [...keys].map(key => {
      const requirement = requirements.get(key);
      const file = files.get(key);
      const recommendation = recommendations.get(key);
      const excluded = exclusions.has(key) || requirement?.enabled === false;
      const required = !excluded && (requirement?.enabled === true || recommendation?.enabled === true);
      const active = file ? { enabled: !file.filename.endsWith(".disabled"), filename: file.filename, version: file.version_number ?? file.version_id ?? null } : null;
      const name = requirement?.displayName || recommendation?.name || file?.display_name || names.get(key) || key;
      names.set(key, name);
      return { key, projectId: key.startsWith("modrinth:") ? key.slice(9) : null, objectId: key.startsWith("local:") ? key.slice(6) : null,
        name, iconUrl: requirement?.iconUrl ?? file?.icon_url ?? null,
        desired: excluded ? "excluded" : required ? "required" : "notRequested",
        source: recommendation ? "recommended" : requirement ? "user" : "dependency", active,
        reason: excluded ? "excluded" : required ? active?.enabled ? "active" : "pending" : active ? "dependency" : "defaultOff",
        canEnable: true, canExclude: !excluded, canInherit: Boolean(requirement || exclusions.has(key)),
      };
    });
    return clone({ profileId: options.profileId, revision: `fixture:${options.profileId}:${revision}`, activeRevisionId: state.activeRevisionId,
      target: options.target(), rows, requested: state });
  }
  function choose(key: string, choice: ModChoice, expectedRevision: string): ModSnapshot {
    const before = snapshot();
    if (before.revision !== expectedRevision) throw new Error("Mod choices changed; refresh before applying this action");
    const row = before.rows.find(item => item.key === key);
    if (!row) throw new Error("Mod key is not present in this snapshot");
    if (!(choice === "include" ? row.canEnable : choice === "exclude" ? row.canExclude : row.canInherit)) throw new Error("Fixture Mod choice is not allowed");
    const state = clone(before.requested);
    state.intent.requirements = state.intent.requirements.filter(item => keyOf(item) !== key);
    state.intent.excludedProjects = state.intent.excludedProjects.filter(id => `modrinth:${id}` !== key);
    if (choice === "include" || (choice === "exclude" && row.objectId)) {
      const requirement: ModRequirement = { projectId: row.projectId, objectId: row.objectId, filename: row.active?.filename ?? null,
        displayName: row.name, iconUrl: row.iconUrl, enabled: choice === "include", origin: "user" };
      state.intent.requirements.push(requirement);
    } else if (choice === "exclude" && row.projectId) state.intent.excludedProjects.push(row.projectId);
    state.pending = true;
    state.preparation = { intentId: "fixture-intent", phase: "queued", reason: null, updatedAt: 1738540800 };
    options.save(state); revision++;
    return snapshot();
  }
  function answer(profileId: string, proposalId: string, expectedRevision: string, action: ModDecisionAction) {
    const decision = proposals.find(item => item.profileId === profileId && item.id === proposalId);
    if (!decision) throw new Error("Mod decision not found");
    if (decision.expectedRevision !== expectedRevision) throw new Error("Mod decision changed; refresh before answering");
    const allowed = decision.phase === "pending" ? ["accept", "defer"] : decision.phase === "deferred" ? ["accept", "defer"] : decision.phase === "accepted" ? ["revoke"] : [];
    if (!allowed.includes(action)) throw new Error("Invalid Mod decision transition");
    decision.phase = action === "accept" ? "accepted" : action === "defer" ? "deferred" : "revoked";
    decision.expectedRevision = `fixture-decision:${++revision}`;
  }
  return { snapshot, choose, answer, touch: () => { revision++; },
    decisions: (profileId: string | null) => clone(proposals.filter(item => !profileId || item.profileId === profileId)),
    setDecisions: (values: DecisionView[]) => { proposals = clone(values); revision++; },
  };
}
