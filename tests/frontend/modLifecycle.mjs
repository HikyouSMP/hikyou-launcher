import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const en = JSON.parse(await readFile(new URL("../../src/locales/en.json", import.meta.url), "utf8"));
const ja = JSON.parse(await readFile(new URL("../../src/locales/ja.json", import.meta.url), "utf8"));
const named = (text, mods = "Sodium") => text.replaceAll("{{mods}}", mods);
const visible = locator => locator.waitFor({ state: "visible" });
const gone = locator => locator.waitFor({ state: "detached" });
const calls = page => page.evaluate(() => window.__HIKYOU_SMOKE__.snapshot().calls);
const snapshot = page => page.evaluate(() => window.__HIKYOU_SMOKE__.snapshot());
const entry = (page, key) => page.locator(`[data-mod-entry="${key}"]`);
async function poll(check, description) {
  const deadline = Date.now() + 10000;
  while (Date.now() < deadline) {
    if (await check()) return;
    await new Promise(resolve => setTimeout(resolve, 50));
  }
  throw new Error(`Timed out: ${description}`);
}
async function openMods(page) {
  await page.locator(".pi").filter({ hasText: "Orchard Fabric" }).click({ button: "right" });
  await page.getByRole("menuitem", { name: en.library.manage, exact: true }).click();
  await page.getByRole("option", { name: en.library.mods, exact: true }).click();
  await visible(page.getByPlaceholder(en.mods.search_placeholder, { exact: true }));
}
async function backToMain(page) {
  await page.getByTitle(en.common.back, { exact: true }).click();
  await visible(page.locator(".profile-scope-overview"));
  await page.getByTitle(en.common.back, { exact: true }).click();
  await visible(page.getByPlaceholder(en.nav.search_placeholder, { exact: true }));
}
async function desired(page, key, value) {
  await poll(async () => (await snapshot(page)).modSnapshot.rows.find(row => row.key === key)?.desired === value, `${key} becomes ${value}`);
}
function assertNoLegacyCommands(recorded) {
  assert.deepEqual(recorded.filter(call => ["get_profile_mods", "get_profile_mod_requests", "toggle_profile_mod", "remove_profile_mod", "remove_requested_mod"].includes(call.command)), []);
}

export async function pendingModRequests(page, capture) {
  await page.setViewportSize({ width: 750, height: 470 });
  await openMods(page);
  const currentKey = "modrinth:smoke-sodium";
  const excludedKey = "modrinth:smoke-global-sodium";
  const pendingKey = "modrinth:smoke-pending";
  const excluded = entry(page, excludedKey);
  const status = page.getByTestId("mod-request-status");
  const query = page.getByPlaceholder(en.mods.search_placeholder, { exact: true });
  await visible(excluded);
  assert.equal((await snapshot(page)).modSnapshot.rows.find(row => row.key === excludedKey).active, null);
  const revision = (await snapshot(page)).modSnapshot.revision;
  await excluded.click();
  await desired(page, excludedKey, "required");
  assert.deepEqual((await calls(page)).filter(call => call.command === "set_profile_mod_choice").at(-1).args,
    { profileId: "smoke-orchard", key: excludedKey, choice: "include", expectedRevision: revision });
  assert.equal((await calls(page)).some(call => call.command === "install_modrinth_mod"), false, "absent exclusion is enabled by identity, not reinstalled by name");
  assert.equal((await snapshot(page)).modSnapshot.rows.find(row => row.key === excludedKey).active, null);
  await visible(status);
  await excluded.hover();
  await excluded.getByTitle(en.mod_lifecycle.exclude, { exact: true }).click();
  await excluded.getByTitle(en.mod_lifecycle.exclude, { exact: true }).click();
  await desired(page, excludedKey, "excluded");
  await visible(excluded);
  await excluded.getByRole("button", { name: en.mod_lifecycle.inherit, exact: true }).click();
  await desired(page, excludedKey, "required");
  assert.equal((await calls(page)).filter(call => call.command === "set_profile_mod_choice").at(-1).args.choice, "inherit");
  await capture("absent-exclusion-and-inheritance");

  await query.fill("sodium");
  const sameName = page.locator('[data-mod-project="smoke-pending"]');
  const renamed = page.locator('[data-mod-project="smoke-sodium"]');
  await visible(sameName.getByTitle(en.mods.install_add, { exact: true }));
  await visible(renamed.getByText(en.mod_lifecycle.reason_active, { exact: true }));
  assert.equal(await renamed.getByTitle(en.mods.install_add, { exact: true }).count(), 0);
  await sameName.getByTitle(en.mods.install_add, { exact: true }).click();
  await desired(page, pendingKey, "required");
  await query.fill("");
  await visible(entry(page, pendingKey));
  assert.equal((await snapshot(page)).modFiles.length, 1, "queued roots and absent exclusions do not claim active files");
  assert.equal(await status.getAttribute("data-phase"), "queued");
  const beforeForeign = (await calls(page)).filter(call => call.command === "get_profile_mod_snapshot").length;
  await page.evaluate(() => window.__HIKYOU_SMOKE__.emitModEvent("mods://prepared", "unrelated-profile"));
  assert.equal((await calls(page)).filter(call => call.command === "get_profile_mod_snapshot").length, beforeForeign);
  await page.evaluate(async () => {
    const fixture = window.__HIKYOU_SMOKE__;
    const state = fixture.snapshot().modRequests;
    state.preparation = { intentId: "blocked-intent", phase: "blocked", reason: "decision_required", updatedAt: 1738540801 };
    fixture.setModRequests(state);
    await fixture.emitModEvent("mods://requests-changed");
  });
  await poll(async () => await status.getAttribute("data-phase") === "blocked", "decision-required marker is displayed");
  assert.ok((await status.innerText()).includes(en.mod_requests.decision_required));
  await page.evaluate(async () => { const { default: i18n } = await import("/src/i18n.ts"); await i18n.changeLanguage("ja"); });
  await visible(status.getByText(ja.mod_requests.current_ready, { exact: true }));
  assert.ok((await status.innerText()).includes(ja.mod_requests.decision_required));
  await capture("decision-required-ja");
  await page.evaluate(async () => { const { default: i18n } = await import("/src/i18n.ts"); await i18n.changeLanguage("en"); });

  await page.evaluate(() => window.__HIKYOU_SMOKE__.failModRequestOnce("get"));
  await page.getByTitle(en.mods.refresh_btn, { exact: true }).click();
  await visible(page.getByRole("alert").filter({ hasText: en.mod_lifecycle.load_failed }));
  await page.getByText(en.mod_failures.details, { exact: true }).click();
  await visible(page.locator('[data-mod-failure="state_unreadable"]'));
  assert.doesNotMatch(await page.locator("body").innerText(), /fixture-private|state\.json/);
  await page.getByRole("button", { name: en.common.retry, exact: true }).click();
  await visible(entry(page, currentKey));
  await gone(page.getByRole("alert"));

  // Hold an old response, publish a ready snapshot, then release the old response.
  await page.evaluate(async () => {
    const fixture = window.__HIKYOU_SMOKE__;
    fixture.holdNextModRequestRead();
    await fixture.emitModEvent("mods://requests-changed");
  });
  await poll(async () => (await snapshot(page)).modReadHeld, "old snapshot read held");
  await page.evaluate(async () => {
    const fixture = window.__HIKYOU_SMOKE__;
    const state = fixture.snapshot();
    state.modRequests.pending = false;
    state.modRequests.preparation = { intentId: "ready-intent", phase: "ready", reason: null, updatedAt: 1738540802 };
    fixture.setModRequests(state.modRequests, [...state.modFiles,
      { filename: "arrived.jar", display_name: "New arrival", project_id: "smoke-pending", icon_url: null, size_bytes: 99 },
      { filename: "global-sodium.jar", display_name: "Sodium", project_id: "smoke-global-sodium", icon_url: null, size_bytes: 100 },
    ]);
    await fixture.emitModEvent("mods://prepared");
  });
  await gone(status);
  await visible(entry(page, pendingKey).getByText(/arrived\.jar/));
  await page.evaluate(() => window.__HIKYOU_SMOKE__.releaseModRequestRead());
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  await gone(status);
  await visible(entry(page, pendingKey).getByText(/arrived\.jar/));
  await page.evaluate(async () => {
    const fixture = window.__HIKYOU_SMOKE__;
    const state = fixture.snapshot().modRequests;
    state.intent.requirements.push({ projectId: null, objectId: "f".repeat(64), filename: "local-request.jar", displayName: "Local request", enabled: true, iconUrl: null, origin: "imported" });
    state.pending = true;
    state.preparation = { intentId: "local-intent", phase: "queued", reason: null, updatedAt: 1738540803 };
    fixture.setModRequests(state);
    await fixture.emitModEvent("mods://requests-changed");
  });
  const localKey = `local:${"f".repeat(64)}`;
  const local = entry(page, localKey);
  await visible(local);
  await local.hover();
  await local.getByTitle(en.mod_lifecycle.exclude, { exact: true }).click();
  await local.getByTitle(en.mod_lifecycle.exclude, { exact: true }).click();
  await desired(page, localKey, "excluded");
  assert.equal((await calls(page)).filter(call => call.command === "set_profile_mod_choice").at(-1).args.key, localKey);
  assertNoLegacyCommands(await calls(page));
  await capture("identity-and-ready-snapshot");
}

export async function modDecisions(page, capture) {
  const search = page.getByPlaceholder(en.nav.search_placeholder, { exact: true });
  const notice = page.locator('[data-mod-decision="synthetic-proposal"]');
  await visible(notice);
  await search.focus();
  for (const event of ["mods://decisions-changed", "mods://requests-changed", "mods://prepared"]) {
    const before = (await calls(page)).filter(call => call.command === "get_mod_decisions").length;
    await page.evaluate(name => window.__HIKYOU_SMOKE__.emitModEvent(name), event);
    await poll(async () => (await calls(page)).filter(call => call.command === "get_mod_decisions").length > before, `${event} refreshes decisions`);
    assert.equal(await search.evaluate(node => document.activeElement === node), true, "notice updates do not focus themselves");
  }
  const wait = notice.getByRole("button", { name: named(en.mod_decisions.defer), exact: true });
  await poll(() => wait.isEnabled(), "known Wait action is enabled");
  await wait.focus();
  assert.equal(await wait.evaluate(node => document.activeElement === node), true, "Wait receives keyboard focus");
  await page.evaluate(async () => {
    const fixture = window.__HIKYOU_SMOKE__;
    fixture.holdNextModDecisionRead();
    await fixture.emitModEvent("mods://decisions-changed");
  });
  await poll(async () => (await snapshot(page)).decisionReadHeld, "background decision read is held");
  assert.equal(await wait.isEnabled(), true, "background reads leave known decision actions enabled");
  assert.equal(await wait.evaluate(node => document.activeElement === node), true, "background refresh preserves Wait focus");
  await wait.press("Enter");
  await gone(notice);
  await page.evaluate(() => window.__HIKYOU_SMOKE__.releaseModDecisionRead());
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  await gone(notice);
  assert.equal((await snapshot(page)).modDecisions[0].phase, "deferred");
  const deferCall = (await calls(page)).filter(call => call.command === "answer_mod_decision").at(-1);
  assert.deepEqual(deferCall.args, { profileId: "smoke-orchard", proposalId: "synthetic-proposal", expectedRevision: "proposal:1", action: "defer" });
  await openMods(page);
  await visible(notice);
  assert.equal(await notice.getAttribute("data-phase"), "deferred");
  assert.equal(await notice.getByRole("button", { name: named(en.mod_decisions.revoke), exact: true }).count(), 0, "waiting is not permission that can be revoked");
  assert.equal(await notice.getByRole("button").count(), 1, "deferred offers accept only");
  const accept = notice.getByRole("button", { name: named(en.mod_decisions.accept), exact: true });
  const beforeAccept = (await calls(page)).filter(call => call.command === "answer_mod_decision").length;
  await accept.focus();
  await accept.press("Enter");
  await visible(notice.getByText(en.mod_decisions.composition_warning, { exact: true }));
  assert.equal((await calls(page)).filter(call => call.command === "answer_mod_decision").length, beforeAccept);
  const confirm = notice.getByRole("button", { name: named(en.mod_decisions.confirm_accept), exact: true });
  assert.equal(await confirm.evaluate(node => document.activeElement === node), true, "explicit confirmation retains its button focus");
  await confirm.press("Enter");
  await poll(async () => await notice.getAttribute("data-phase") === "accepted", "deferred permission is accepted");
  await backToMain(page);
  await gone(notice);
  await openMods(page);
  await visible(notice);
  assert.equal(await notice.getAttribute("data-phase"), "accepted", "accepted permission survives panel remount");
  await notice.getByRole("button", { name: named(en.mod_decisions.revoke), exact: true }).click();
  await notice.getByRole("button", { name: named(en.mod_decisions.confirm_revoke), exact: true }).click();
  await gone(notice);
  assert.equal((await snapshot(page)).modDecisions[0].phase, "revoked");
  await backToMain(page);

  // A new pending proposal can also be accepted directly on the main surface.
  await page.evaluate(async () => {
    const fixture = window.__HIKYOU_SMOKE__;
    const proposal = fixture.snapshot().modDecisions[0];
    fixture.setModDecisions([{ ...proposal, id: "pending-direct", phase: "pending", expectedRevision: "direct:1" }]);
    await fixture.emitModEvent("mods://decisions-changed");
  });
  const direct = page.locator('[data-mod-decision="pending-direct"]');
  await visible(direct);
  await direct.getByRole("button", { name: named(en.mod_decisions.accept), exact: true }).click();
  await visible(direct.getByText(en.mod_decisions.composition_warning, { exact: true }));
  await capture("pending-confirmation");
  await direct.getByRole("button", { name: named(en.mod_decisions.confirm_accept), exact: true }).press("Enter");
  await gone(direct);
  const recorded = await calls(page);
  assert.equal((await snapshot(page)).modDecisions[0].phase, "accepted");
  assert.deepEqual(recorded.filter(call => call.command === "answer_mod_decision").map(call => call.args.action), ["defer", "accept", "revoke", "accept"]);
  assert.equal(recorded.some(call => /start_webview_login|launch_game|open_world|upload|open_launch_error_report/.test(call.command)), false, "decision UI never starts login, opens worlds, launches games, or sends diagnostics");
  assertNoLegacyCommands(recorded);
}
