import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import ts from "typescript";

const source = await readFile(new URL("../../src/modRequests.ts", import.meta.url), "utf8");
const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ESNext } }).outputText;
const { requestedKey, projectFile, projectRequirement, requestStatus, pendingRequirements } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`);
const requirement = (extra = {}) => ({ projectId: "exact-id", objectId: null, filename: null, displayName: "Sodium", enabled: true, iconUrl: null, origin: "user", ...extra });
const state = (extra = {}) => ({ intent: { schema: 1, profileId: "profile", requirements: [requirement()], excludedProjects: [] }, activeRevisionId: "ready", pending: true, preparation: null, ...extra });
const file = (extra = {}) => ({ filename: "sodium.jar", display_name: "Sodium", project_id: "other-id", size_bytes: 1, icon_url: null, ...extra });

test("request keys use provider or content identity, never filenames", () => {
  assert.equal(requestedKey(requirement()), "modrinth:exact-id");
  assert.equal(requestedKey(requirement({ projectId: null, objectId: "a".repeat(64) })), `local:${"a".repeat(64)}`);
  assert.equal(requestedKey(requirement({ projectId: null, filename: "sodium.jar" })), null);
});
test("same names are not installed identity and renamed projects retain identity", () => {
  assert.equal(projectFile([file()], "exact-id"), undefined);
  assert.equal(projectFile([file({ project_id: null })], "exact-id"), undefined);
  assert.equal(projectFile([file({ project_id: "exact-id", display_name: "Renamed", filename: "different.jar" })], "exact-id").filename, "different.jar");
  assert.equal(projectRequirement(state(), "other-id"), undefined);
});
test("unresolved roots remain pending without changing current files", () => {
  const files = [file()];
  assert.deepEqual(pendingRequirements(state(), files), [requirement()]);
  assert.deepEqual(files, [file()]);
});
test("desired toggles remain pending while actual filenames are unchanged", () => {
  const desired = state({ intent: { requirements: [requirement({ enabled: false })] } });
  const files = [file({ project_id: "exact-id" })];
  assert.equal(pendingRequirements(desired, files).length, 1);
  assert.equal(files[0].filename, "sodium.jar");
  assert.equal(pendingRequirements(desired, [file({ project_id: "exact-id", filename: "sodium.jar.disabled" })]).length, 0);
});
test("local requests never claim content identity from filename equality", () => {
  const local = requirement({ projectId: null, objectId: "a".repeat(64), filename: "sodium.jar" });
  assert.deepEqual(pendingRequirements(state({ intent: { requirements: [local] } }), [file()]), [local]);
});
test("backend lifecycle controls visibility, not file presence", () => {
  assert.equal(requestStatus(null), null);
  assert.equal(requestStatus(state({ pending: false })), null);
  for (const phase of ["queued", "preparing", "blocked"]) {
    assert.equal(requestStatus(state({ preparation: { phase }, pending: false })), phase);
  }
  assert.equal(requestStatus(state({ preparation: { phase: "ready" } })), "pending");
  assert.deepEqual(pendingRequirements(state({ pending: false, preparation: { phase: "ready" } }), []), []);
});
