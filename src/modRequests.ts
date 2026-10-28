import type { ModFile } from "./types";

export interface ModRequirement {
  projectId: string | null;
  objectId: string | null;
  filename: string | null;
  enabled: boolean;
  displayName: string | null;
  iconUrl: string | null;
  origin: "user" | "imported" | "recommended";
}

export interface RequestedState {
  intent: {
    schema: 1;
    profileId: string;
    requirements: ModRequirement[];
    excludedProjects: string[];
  };
  activeRevisionId: string | null;
  pending: boolean;
  preparation: {
    intentId: string;
    phase: "queued" | "preparing" | "ready" | "blocked";
    reason: "dependencies_incompatible" | "provider_unavailable" | "files_changed" | "preparation_failed" | null;
    updatedAt: number;
  } | null;
}

// Cached metadata can lack provider identity. Never reconstruct it from names.
export type IdentifiedModFile = ModFile & { project_id?: string | null };

export function requestedKey(requirement: ModRequirement): string | null {
  if (requirement.projectId) return `modrinth:${requirement.projectId}`;
  if (requirement.objectId) return `local:${requirement.objectId}`;
  return null;
}

export function projectRequirement(state: RequestedState | null, projectId: string): ModRequirement | undefined {
  return state?.intent.requirements.find((requirement) => requirement.projectId === projectId);
}

export function projectFile(files: IdentifiedModFile[], projectId: string): IdentifiedModFile | undefined {
  return files.find((file) => file.project_id === projectId);
}

export function requestStatus(state: RequestedState | null): "queued" | "preparing" | "blocked" | "pending" | null {
  if (!state) return null;
  const phase = state.preparation?.phase;
  if (phase === "queued" || phase === "preparing" || phase === "blocked") return phase;
  return state.pending ? "pending" : null;
}

export function pendingRequirements(state: RequestedState | null, files: IdentifiedModFile[]): ModRequirement[] {
  if (!state || !requestStatus(state)) return [];
  return state.intent.requirements.filter((requirement) => {
    // ModFile does not expose local content hashes; filename equality is not identity.
    if (!requirement.projectId) return true;
    const file = projectFile(files, requirement.projectId);
    return !file || (!file.filename.endsWith(".disabled")) !== requirement.enabled;
  });
}

export function requirementName(requirement: ModRequirement): string {
  return requirement.displayName || requirement.filename || requirement.projectId || requirement.objectId?.slice(0, 12) || "Mod";
}
