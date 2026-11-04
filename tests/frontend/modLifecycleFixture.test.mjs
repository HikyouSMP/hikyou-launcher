import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import ts from "typescript";
import { runFrontendSmoke } from "./smoke.mjs";

const source = await readFile(new URL("./modLifecycleFixture.ts", import.meta.url), "utf8");
const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ESNext } }).outputText;
const { createModLifecycleFixture } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`);
function fixture() {
  let state = { intent: { schema: 1, profileId: "synthetic", requirements: [], excludedProjects: ["global-sodium"] }, activeRevisionId: "ready", pending: false, preparation: null };
  return createModLifecycleFixture({ profileId: "synthetic", requested: () => structuredClone(state), save: value => { state = value; },
    files: () => [{ project_id: "another-sodium", filename: "same-name.jar", display_name: "Sodium", icon_url: null, size_bytes: 1 }],
    target: () => ({ mcVersion: "1.22", loader: "fabric", loaderVersion: null }),
    recommendations: [{ projectId: "global-sodium", name: "Sodium", enabled: true }],
  });
}
test("synthetic snapshot keeps absent global exclusion and includes by exact identity, not matching names", () => {
  const model = fixture();
  const before = model.snapshot();
  const key = "modrinth:global-sodium";
  const excluded = before.rows.find(row => row.key === key);
  assert.equal(excluded.active, null);
  assert.equal(excluded.desired, "excluded");
  const after = model.choose(key, "include", before.revision);
  assert.equal(after.rows.find(row => row.key === key).desired, "required");
  assert.equal(after.rows.find(row => row.key === key).active, null);
  assert.deepEqual(after.rows.find(row => row.key === "modrinth:another-sodium"), before.rows.find(row => row.key === "modrinth:another-sodium"));
  assert.throws(() => model.choose(key, "exclude", before.revision), /Mod choices changed/);
  const excludedAgain = model.choose(key, "exclude", after.revision);
  assert.equal(excludedAgain.rows.find(row => row.key === key).desired, "excluded");
  const inherited = model.choose(key, "inherit", excludedAgain.revision);
  assert.equal(inherited.rows.find(row => row.key === key).desired, "required");
  assert.deepEqual(inherited.requested.intent.requirements, []);
});
test("synthetic decision fixture enforces pending/deferred/accepted/revoked transitions and exact revision tokens", () => {
  const model = fixture();
  model.setDecisions([{ id: "proposal", profileId: "synthetic", target: "1.22", phase: "pending", omitted: [{ key: "modrinth:global-sodium", name: "Sodium" }], baselineTarget: "1.21", expectedRevision: "d1" }]);
  model.answer("synthetic", "proposal", "d1", "defer");
  let decision = model.decisions("synthetic")[0];
  assert.equal(decision.phase, "deferred");
  assert.throws(() => model.answer("synthetic", "proposal", decision.expectedRevision, "revoke"), /Invalid Mod decision transition/);
  assert.throws(() => model.answer("synthetic", "proposal", "d1", "accept"), /Mod decision changed/);
  model.answer("synthetic", "proposal", decision.expectedRevision, "accept");
  decision = model.decisions(null)[0];
  assert.equal(decision.phase, "accepted");
  model.answer("synthetic", "proposal", decision.expectedRevision, "revoke");
  assert.equal(model.decisions(null)[0].phase, "revoked");
  assert.deepEqual(model.decisions("unrelated"), []);
});
test("smoke scenario selection rejects mistakes before browser launch or filesystem preparation", async () => {
  const chromium = { launch: () => assert.fail("No browser may launch in a pure test") };
  for (const scenarioIds of [[], ["missing-scenario"]]) {
    await assert.rejects(runFrontendSmoke({ chromium, scenarioIds }), /Unknown or empty frontend smoke scenario/);
  }
});
