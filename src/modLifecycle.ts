import type { RequestedState } from "./modRequests";

export interface ModLifecycleTarget {
  mcVersion: string;
  loader: string;
  loaderVersion: string | null;
}

export type ModChoice = "include" | "exclude" | "inherit";
export interface ModChoiceRow {
  key: string;
  projectId: string | null;
  objectId: string | null;
  name: string;
  iconUrl: string | null;
  desired: "required" | "excluded" | "notRequested";
  source: "recommended" | "user" | "legacyUnknown" | "dependency";
  active: { enabled: boolean; version: string | null; filename: string } | null;
  reason: "excluded" | "legacyExcluded" | "pending" | "active" | "defaultOff" | "dependency";
  canEnable: boolean;
  canExclude: boolean;
  canInherit: boolean;
}

export interface ModSnapshot {
  profileId: string;
  revision: string;
  activeRevisionId: string | null;
  target: ModLifecycleTarget | null;
  rows: ModChoiceRow[];
  requested: RequestedState;
}

export interface DecisionView {
  id: string;
  profileId: string;
  target: string;
  phase: "pending" | "deferred" | "accepted" | "revoked";
  omitted: { key: string; name: string }[];
  baselineTarget: string | null;
  expectedRevision: string;
}
export type ModDecisionAction = "accept" | "defer" | "revoke";

export const modLifecycleEvents = ["mods://decisions-changed", "mods://requests-changed", "mods://prepared"] as const;

export function toggleChoice(row: ModChoiceRow): ModChoice | null {
  if (row.desired === "excluded" || row.reason === "legacyExcluded" || row.reason === "defaultOff" || (row.desired === "notRequested" && !row.active?.enabled)) {
    return row.canEnable ? "include" : null;
  }
  return row.canExclude ? "exclude" : null;
}

export function choiceAllowed(row: ModChoiceRow, choice: ModChoice): boolean {
  return choice === "include" ? row.canEnable : choice === "exclude" ? row.canExclude : row.canInherit;
}

export function decisionActions(phase: DecisionView["phase"]): ModDecisionAction[] {
  if (phase === "pending") return ["defer", "accept"];
  if (phase === "deferred") return ["accept"];
  if (phase === "accepted") return ["revoke"];
  return [];
}

export function visibleDecisions(decisions: DecisionView[], panel: boolean): DecisionView[] {
  const visible = decisions.filter((decision) => panel ? decision.phase !== "revoked" : decision.phase === "pending");
  return panel ? visible : visible.slice(0, 1);
}
