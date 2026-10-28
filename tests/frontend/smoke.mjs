import assert from "node:assert/strict";
import { readFile, mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

const en = JSON.parse(await readFile(new URL("../../src/locales/en.json", import.meta.url), "utf8"));
const ja = JSON.parse(await readFile(new URL("../../src/locales/ja.json", import.meta.url), "utf8"));
const japanese = "\u65e5\u672c\u8a9e";

async function poll(check, description, timeout = 10000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await check()) return;
    await new Promise((resolve) => setTimeout(resolve, 80));
  }
  throw new Error(`Timed out: ${description}`);
}
const count = (locator, expected) => poll(async () => await locator.count() === expected, `${locator} count = ${expected}`);
const visible = (locator) => locator.waitFor({ state: "visible" });
const mainSearch = (page) => page.getByPlaceholder(en.nav.search_placeholder, { exact: true });

async function typeSearch(page, value) {
  const search = mainSearch(page);
  await search.click();
  if (await search.inputValue()) {
    await search.press("ControlOrMeta+A");
    await search.press("Backspace");
  }
  if (value) await search.pressSequentially(value);
  return search;
}

async function mainKeyboard(page, capture) {
  await count(page.locator(".pi"), 3);
  assert.ok(await page.locator(".pi .chip").count() > 0);
  assert.equal(await page.locator(".pi .chip").evaluateAll((chips) => chips.every((chip) => {
    const style = getComputedStyle(chip);
    return parseFloat(style.borderTopLeftRadius) >= chip.getBoundingClientRect().height / 2;
  })), true, "main profile badges are fully rounded pills");
  await typeSearch(page, "Orchard");
  await count(page.locator(".pi"), 1);
  assert.match(await page.locator(".pi").innerText(), /Orchard Fabric/);
  await page.keyboard.press("Escape");
  await count(page.locator(".pi"), 3);
  assert.equal(await mainSearch(page).inputValue(), "");
  await typeSearch(page, "fabric");
  await count(page.locator(".pi"), 1);
  assert.match(await page.locator(".pi").innerText(), /Orchard Fabric/);
  // The profile is initially selected; move to a create candidate and back.
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("ControlOrMeta+k");
  await visible(page.locator(".pcp-modal"));
  assert.equal(await page.locator(".pcp-title-input").inputValue(), "Orchard Fabric");
  await page.keyboard.press("Escape");
  await count(page.locator(".pcp-modal"), 0);
  await page.keyboard.press("Escape");
  await count(page.locator(".pi"), 3);
  await typeSearch(page, "latest");
  const snapshot = page.getByRole("button", { name: "Vanilla 24w33a", exact: true });
  const release = page.getByRole("button", { name: "Vanilla 1.21.1", exact: true });
  await count(snapshot, 0);
  await visible(release);
  await page.keyboard.press("Tab");
  await visible(snapshot);
  await count(release, 0);
  await visible(page.locator("[data-tauri-drag-region] .chip").filter({ hasText: en.nav.snapshot_channel }));
  await poll(() => page.evaluate(() => window.__HIKYOU_SMOKE__.snapshot().settings.game.versionChannel === "snapshot"), "snapshot channel persisted");
  assert.equal(await mainSearch(page).inputValue(), "latest");
  await count(page.locator(".search-suggestion"), 0);
  await typeSearch(page, "snapshot");
  const suggestion = page.locator("[data-command-footer]").getByRole("button", { name: `Tab ${en.searchSuggestions.toReleases}`, exact: true });
  await visible(suggestion);
  await page.setViewportSize({ width: 750, height: 470 });
  const inlineHint = page.locator("[data-search-input-shell] .search-suggestion");
  await visible(inlineHint);
  assert.equal(await inlineHint.innerText(), "Tab");
  const inlinePlacement = await inlineHint.evaluate(element => {
    const hint = element.getBoundingClientRect();
    const shell = element.closest("[data-search-input-shell]");
    const mirror = shell.querySelector(".caret-mirror");
    const range = document.createRange();
    range.setStart(mirror.firstChild, mirror.textContent.length); range.collapse(true);
    const caret = range.getBoundingClientRect();
    return { gap: hint.left - caret.left, inside: hint.right <= shell.getBoundingClientRect().right };
  });
  assert.ok(inlinePlacement.gap >= 6 && inlinePlacement.gap <= 12 && inlinePlacement.inside, "shortcut follows the actual caret, not the input's right edge");
  await mainSearch(page).press("ArrowLeft");
  assert.equal(await inlineHint.isVisible(), false, "editing inside the query never overlays existing text");
  await mainSearch(page).press("End");
  await visible(inlineHint);
  const placement = await suggestion.evaluate(element => {
    const item = element.getBoundingClientRect();
    const footer = element.closest("[data-command-footer]").getBoundingClientRect();
    return { centerOffset: Math.abs(item.x + item.width / 2 - (footer.x + footer.width / 2)), contained: item.left >= footer.left && item.right <= footer.right && item.top >= footer.top && item.bottom <= footer.bottom };
  });
  assert.ok(placement.centerOffset < 1 && placement.contained, "suggestion is centered and contained at native window size");
  await capture("snapshot-suggestion");
  await page.setViewportSize({ width: 1040, height: 760 });
  await suggestion.focus();
  assert.equal(await suggestion.evaluate(element => document.activeElement === element), true, "suggestions retain keyboard focus");
  await page.keyboard.press("Enter");
  await count(release, 0);
  await count(snapshot, 0);
  await poll(() => page.evaluate(() => window.__HIKYOU_SMOKE__.snapshot().settings.game.versionChannel === "release"), "suggestion selects release channel without unrelated snapshot-query results");
  assert.equal(await mainSearch(page).evaluate(element => document.activeElement === element), true);
  assert.equal(await page.evaluate(() => window.__HIKYOU_SMOKE__.snapshot().calls.some(call => call.command === "launch_game")), false, "activating the suggestion must not launch a profile");
  const toSnapshots = page.locator("[data-command-footer]").getByRole("button", { name: `Tab ${en.searchSuggestions.toSnapshots}`, exact: true });
  await visible(toSnapshots);
  await inlineHint.click();
  await visible(snapshot);
  await count(release, 0);
  await poll(() => page.evaluate(() => window.__HIKYOU_SMOKE__.snapshot().settings.game.versionChannel === "snapshot"), "suggestion click persists snapshot channel");
  await page.keyboard.press("ControlOrMeta+,");
  await visible(page.locator(".settings-dropdown-wrap").first());
  await count(page.getByRole("switch", { name: en.nav.snapshot_channel, exact: true }), 0);
  await count(page.locator(".search-suggestion"), 0);
  await page.keyboard.press("Tab");
  assert.equal(await page.evaluate(() => window.__HIKYOU_SMOKE__.snapshot().settings.game.versionChannel), "snapshot", "settings Tab must not switch channel");
  await page.getByTitle(en.common.back, { exact: true }).click();
  await mainSearch(page).focus();
  await page.keyboard.press("Tab");
  await count(snapshot, 0);
  await count(release, 0);
  await poll(() => page.evaluate(() => window.__HIKYOU_SMOKE__.snapshot().settings.game.versionChannel === "release"), "second Tab selects releases");
  assert.equal(await page.evaluate(() => window.__HIKYOU_SMOKE__.snapshot().calls.some(call => call.command === "search_modrinth_modpacks")), false);
  await page.keyboard.press("Escape");
  await count(page.locator(".pi"), 3);
  await count(page.locator(".search-suggestion"), 0);
  await capture("main-keyboard");
}

async function searchSelection(page, capture) {
  await page.setViewportSize({ width: 750, height: 470 });
  await page.mouse.move(2, 2);
  const search = mainSearch(page);
  const rows = page.locator("[data-command-selected]");
  const selected = page.locator('[data-command-selected="true"]');
  await count(rows, 3);
  await count(selected, 1);
  await search.focus();
  const selectionColor = await search.evaluate(element => {
    const log = document.createElement("pre");
    log.className = "log-body";
    document.body.append(log);
    const colors = [element, document.querySelector(".pi p"), log]
      .map(node => getComputedStyle(node, "::selection").backgroundColor);
    log.remove();
    return colors;
  });
  assert.deepEqual(selectionColor, Array(3).fill("rgba(255, 255, 255, 0.24)"), "search, ordinary text and logs share a neutral selection color");

  async function editAndSample(value, hasResults = true) {
    const frames = await search.evaluate(async (element, value) => {
      element.value = value;
      element.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertReplacementText" }));
      const frames = [];
      for (let frame = 0; frame < 8; frame++) {
        await new Promise(resolve => requestAnimationFrame(resolve));
        const rows = [...document.querySelectorAll("[data-command-selected]")];
        const selected = rows.filter(row => row.dataset.commandSelected === "true");
        const background = rows[0] ? getComputedStyle(rows[0]).backgroundColor : null;
        const highlighted = rows.flatMap((row, index) => {
          const color = getComputedStyle(row).backgroundColor;
          return color !== "rgba(0, 0, 0, 0)" && color !== "transparent" ? [index] : [];
        });
        const bounds = rows[0]?.getBoundingClientRect();
        const listBounds = rows[0]?.closest(".sb")?.getBoundingClientRect();
        frames.push({ count: rows.length, selected: selected.map(row => rows.indexOf(row)), highlighted, background,
          firstVisible: !bounds || (bounds.top >= listBounds.top && bounds.bottom <= listBounds.bottom) });
      }
      return frames;
    }, value);
    for (const [index, frame] of frames.entries()) {
      assert.deepEqual(frame.selected, hasResults ? [0] : [], `query ${JSON.stringify(value)} frame ${index}: only the first result is selected`);
      assert.deepEqual(frame.highlighted, hasResults ? [0] : [], `query ${JSON.stringify(value)} frame ${index}: no stale hover or selection remains below the first result`);
      assert.equal(frame.firstVisible, true, `query ${JSON.stringify(value)} frame ${index}: first result is visible`);
      if (hasResults) {
        assert.ok(frame.count > 0);
        const alpha = Number(frame.background.match(/^rgba\(255, 255, 255, ([\d.]+)\)$/)?.[1]);
        assert.ok(alpha >= 0.058 && alpha <= 0.087, `query ${JSON.stringify(value)} frame ${index}: selection background is fully present, got ${frame.background}`);
      } else assert.equal(frame.count, 0);
    }
  }

  await search.press("ArrowDown");
  assert.match(await selected.innerText(), /Alpine Vanilla/);
  await rows.nth(1).hover();
  await editAndSample("a");
  await editAndSample("Orchard");
  await editAndSample("no-results-qzx", false);
  await editAndSample("latest");
  await search.press("ArrowDown");
  await search.press("ArrowDown");
  await search.press("ArrowDown");
  assert.doesNotMatch(await selected.innerText(), /^Vanilla 1\.21\.1$/);
  await editAndSample("1.21");
  // Keep the index at zero while replacing the entire result set and scroll position.
  await editAndSample("1.20");
  await editAndSample("latest vanilla");
  await search.press("ArrowDown");
  assert.match(await selected.innerText(), /Vanilla 1\.20\.1/);
  await editAndSample("vanilla 1.21");
  await editAndSample("no-results-qzx", false);
  await editAndSample("");
  await rows.nth(2).hover();
  await poll(async () => await rows.nth(2).evaluate(element => getComputedStyle(element).backgroundColor !== "rgba(0, 0, 0, 0)"), "real pointer movement still highlights the hovered result");
  await search.press("ArrowDown");
  assert.match(await selected.innerText(), /Alpine Vanilla/, "keyboard navigation still works after search resets");
  assert.deepEqual(await rows.evaluateAll(elements => elements.flatMap((element, index) => getComputedStyle(element).backgroundColor !== "rgba(0, 0, 0, 0)" ? [index] : [])), [1], "keyboard navigation clears pointer hover");
  await capture("stable-selection");
}

async function modpackAliases(page, capture) {
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await page.setViewportSize({ width: 750, height: 470 });
  const badge = page.locator("[data-tauri-drag-region] .chip").filter({ hasText: "Modpack" });
  await count(page.locator("[data-tauri-drag-region] .chip"), 0);
  await typeSearch(page, "snap");
  await visible(page.getByRole("button", { name: `Tab ${en.searchSuggestions.toSnapshots}`, exact: true }));
  await typeSearch(page, "sn");
  await count(page.locator(".search-suggestion"), 0);
  await typeSearch(page, "mp");
  await mainSearch(page).press("Home");
  await mainSearch(page).press("Space");
  assert.equal(await mainSearch(page).inputValue(), " mp");
  await count(badge, 0);
  for (const alias of ["modpack", "mp"]) {
    const search = await typeSearch(page, alias);
    await visible(page.getByRole("button", { name: `Space ${en.searchSuggestions.searchModpacks}`, exact: true }));
    await count(page.getByRole("button", { name: "Vanilla 1.21.1", exact: true }), 0);
    await count(page.getByText(en.profile.no_match.replace("{{query}}", alias), { exact: true }), 0);
    if (alias === "mp") await capture("modpack-alias");
    await search.press("Space");
    await visible(badge);
    assert.equal(await badge.evaluate(element => getComputedStyle(element).color), "rgb(155, 152, 144)", "Modpack uses neutral secondary text, not primary green");
    assert.equal(await badge.evaluate(element => getComputedStyle(element, "::before").backgroundColor), "rgba(255, 255, 255, 0.06)");
    assert.equal(await mainSearch(page).inputValue(), "", "mode entry consumes the command and separator");
    await count(page.locator(".search-suggestion"), 0);
    await page.keyboard.press(alias === "mp" ? "Backspace" : "Escape");
    await count(badge, 0);
  }
  await typeSearch(page, "modpack");
  await page.getByRole("button", { name: `Space ${en.searchSuggestions.searchModpacks}`, exact: true }).click();
  await visible(badge);
  assert.equal(await mainSearch(page).inputValue(), "");
  await typeSearch(page, "cool pack");
  await poll(() => page.evaluate(() => window.__HIKYOU_SMOKE__.snapshot().calls.some(call =>
    call.command === "search_modrinth_modpacks" && call.args.query === "cool pack")), "scoped search sends only the actual Modpack query");
  await page.keyboard.press("Escape");
  await visible(badge);
  assert.equal(await mainSearch(page).inputValue(), "");
  await typeSearch(page, "x");
  await mainSearch(page).press("Backspace");
  assert.equal(await mainSearch(page).inputValue(), "");
  await visible(badge);
  await mainSearch(page).dispatchEvent("keydown", { key: "Backspace", code: "Backspace", repeat: true });
  await visible(badge);
  await mainSearch(page).press("Backspace");
  await count(badge, 0);
  await count(page.locator("[data-tauri-drag-region] .chip"), 0);
  await page.keyboard.press("Tab");
  const snapshotChip = page.locator("[data-tauri-drag-region] .chip").filter({ hasText: en.nav.snapshot_channel });
  await visible(snapshotChip);
  const motion = await snapshotChip.evaluate(element => {
    const animations = element.getAnimations({ subtree: true });
    const sample = time => {
      for (const animation of animations) { animation.pause(); animation.currentTime = time; }
      return {
        background: new DOMMatrixReadOnly(getComputedStyle(element, "::before").transform).a,
        origin: parseFloat(getComputedStyle(element, "::before").transformOrigin),
        label: new DOMMatrixReadOnly(getComputedStyle(element.querySelector(".search-mode-chip-label")).transform).m41,
        labelLeft: element.querySelector(".search-mode-chip-label").getBoundingClientRect().left,
        labelRight: element.querySelector(".search-mode-chip-label").getBoundingClientRect().right,
        left: element.getBoundingClientRect().left,
        right: element.getBoundingClientRect().right,
        clipped: getComputedStyle(element).overflowX === "hidden",
        width: element.getBoundingClientRect().width,
      };
    };
    return {
      count: animations.length,
      timing: animations.map(animation => {
        const timing = animation.effect.getTiming();
        return { duration: timing.duration, delay: timing.delay, easing: animation.effect.getKeyframes()[0].easing };
      }),
      start: sample(0), middle: sample(80), end: sample(300),
    };
  });
  assert.equal(motion.count, 2);
  assert.deepEqual(motion.timing[0], motion.timing[1], "background and text must share one timing curve without a phase offset");
  assert.equal(motion.timing[0].duration, 250);
  assert.equal(motion.timing[0].delay, 0);
  assert.ok(motion.timing[0].easing.startsWith("linear("), "the supported browser uses the sampled spring curve");
  assert.ok(motion.start.background < motion.middle.background && motion.middle.background < motion.end.background);
  assert.ok(motion.start.label > motion.middle.label && motion.middle.label > motion.end.label, "text slides from right to left");
  assert.ok(Math.abs(motion.start.origin - motion.start.width) < 0.1, "background expands from the fixed right edge");
  assert.ok(motion.start.labelLeft >= motion.start.right - 0.1, "text starts outside the frame on the right");
  assert.ok(motion.end.labelLeft >= motion.end.left && motion.end.labelRight <= motion.end.right, "text ends inside the frame");
  assert.equal(motion.start.clipped, true, "the frame clips the sliding text");
  assert.equal(motion.end.background, 1);
  assert.equal(motion.end.label, 0);
  assert.ok(Math.abs((motion.start.label - motion.middle.label) / motion.start.label - motion.middle.background) < 0.0001, "text and background remain synchronized mid-motion");
  assert.equal(motion.start.width, motion.end.width, "animation must not resize the chip layout or distort its text");
  await snapshotChip.evaluate(element => { for (const animation of element.getAnimations({ subtree: true })) animation.currentTime = 80; });
  await capture("snapshot-chip-mid", { animations: "allow" });
  await snapshotChip.evaluate(element => { for (const animation of element.getAnimations({ subtree: true })) animation.currentTime = 300; });
  await capture("snapshot-chip-complete");
  await page.keyboard.press("Tab");
  await count(page.locator("[data-tauri-drag-region] .chip"), 0);
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.keyboard.press("Tab");
  await visible(snapshotChip);
  assert.equal(await snapshotChip.evaluate(element => element.getAnimations({ subtree: true }).length), 0, "reduced-motion disables both animations");
  await page.keyboard.press("Tab");
  await count(snapshotChip, 0);
}

async function settingsLocale(page, capture) {
  await page.keyboard.press("ControlOrMeta+,");
  await visible(page.locator(".settings-dropdown-wrap").first());
  const language = page.locator(".settings-dropdown-wrap .log-dropdown-trigger").filter({ hasText: "English" });
  await language.click();
  await page.getByRole("button", { name: japanese, exact: true }).click();
  await visible(page.getByText(ja.settings.language_label, { exact: true }).first());
  await poll(() => page.evaluate(() => window.__HIKYOU_SMOKE__.snapshot().settings.ui.locale === "ja"), "Japanese locale persisted to memory fixture");
  await capture("settings-ja");
  await page.getByTitle(ja.common.back, { exact: true }).click();
  await visible(page.getByPlaceholder(ja.nav.search_placeholder, { exact: true }));
  await page.getByPlaceholder(ja.nav.search_placeholder, { exact: true }).fill("snapshot");
  await visible(page.getByRole("button", { name: `Tab ${ja.searchSuggestions.toSnapshots}`, exact: true }));
  await page.setViewportSize({ width: 750, height: 470 });
  await capture("search-suggestion-ja");
  await page.setViewportSize({ width: 1040, height: 760 });
  await page.getByPlaceholder(ja.nav.search_placeholder, { exact: true }).fill("");
  await count(page.locator(".search-suggestion"), 0);
  await page.getByTitle(ja.common.settings, { exact: true }).click();
  await page.locator(".settings-dropdown-wrap .log-dropdown-trigger").filter({ hasText: japanese }).click();
  await page.getByRole("button", { name: "English", exact: true }).click();
  await visible(page.getByText(en.settings.language_label, { exact: true }).first());
  await poll(() => page.evaluate(() => window.__HIKYOU_SMOKE__.snapshot().settings.ui.locale === "en"), "English locale persisted to memory fixture");
  await page.getByTitle(en.common.back, { exact: true }).click();
  await visible(mainSearch(page));
}

async function profileLibrary(page, capture) {
  const row = page.locator(".pi").filter({ hasText: "Orchard Fabric" });
  await row.click({ button: "right" });
  await page.getByRole("menuitem", { name: en.profile.edit_settings, exact: true }).click();
  await visible(page.locator(".pcp-modal"));
  assert.equal(await page.locator(".pcp-title-input").inputValue(), "Orchard Fabric");
  await capture("profile-inspector");
  await page.keyboard.press("Escape");
  await count(page.locator(".pcp-modal"), 0);
  await row.click({ button: "right" });
  await page.getByRole("menuitem", { name: en.library.manage, exact: true }).click();
  const overview = page.locator(".profile-scope-overview");
  await visible(overview);
  await page.locator("header[data-tauri-drag-region]").first().hover({ position: { x: 100, y: 10 } });
  await overview.focus();
  const navigation = await overview.getByRole("option").evaluateAll((options, label) => ({
    selected: options.findIndex((option) => option.getAttribute("aria-selected") === "true"),
    target: options.findIndex((option) => option.textContent.trim() === label),
    count: options.length,
  }), en.library.worlds);
  assert.ok(navigation.selected >= 0 && navigation.target >= 0, "library must expose selected and Worlds options");
  const steps = (navigation.target - navigation.selected + navigation.count) % navigation.count;
  for (let index = 0; index < steps; index++) await page.keyboard.press("ArrowDown");
  const worlds = overview.getByRole("option", { name: en.library.worlds, exact: true });
  await poll(async () => await worlds.getAttribute("aria-selected") === "true", "Worlds selected with arrow keys");
  await page.keyboard.press("Enter");
  await page.getByRole("button", { name: /Orchard World/ }).click();
  await visible(page.getByText("orchard-recipes.zip", { exact: true }));
  await capture("library-datapacks");
  await page.locator(".library-surface-heading").getByTitle(en.common.back, { exact: true }).click();
  await visible(page.getByRole("button", { name: /Orchard World/ }));
  await page.getByTitle(en.common.back, { exact: true }).click();
  await visible(overview);
  await page.getByTitle(en.common.back, { exact: true }).click();
  await visible(mainSearch(page));
}

async function pendingModRequests(page, capture) {
  await page.setViewportSize({ width: 750, height: 470 });
  const calls = () => page.evaluate(() => window.__HIKYOU_SMOKE__.snapshot().calls);
  const reads = async () => (await calls()).filter(call => call.command === "get_profile_mod_requests").length;
  const row = page.locator(".pi").filter({ hasText: "Orchard Fabric" });
  await row.click({ button: "right" });
  await page.getByRole("menuitem", { name: en.library.manage, exact: true }).click();
  await page.getByRole("option", { name: en.library.mods, exact: true }).click();
  const current = page.locator('[data-mod-entry="sodium-fixture.jar"]');
  const status = page.getByTestId("mod-request-status");
  const query = page.getByPlaceholder(en.mods.search_placeholder, { exact: true });
  const pendingRoot = () => status.locator('[data-requested-key="modrinth:smoke-pending"]');
  await visible(current);
  await count(status, 0);
  await query.fill("sodium");
  const sameName = page.locator(".h-11").filter({ has: page.getByText("Sodium", { exact: true }) });
  const renamed = page.locator(".h-11").filter({ hasText: "Renamed project" });
  await visible(sameName.getByTitle(en.mods.install_add, { exact: true }));
  await visible(renamed.getByText(en.mod_requests.requested, { exact: true }));
  assert.equal(await renamed.getByTitle(en.mods.install_add, { exact: true }).count(), 0, "provider ID matches despite different name and slug");
  const initialReads = await reads();
  await sameName.getByTitle(en.mods.install_add, { exact: true }).click();
  await visible(pendingRoot());
  assert.equal(await status.getAttribute("data-phase"), "queued");
  await visible(sameName.getByText(en.mod_requests.requested, { exact: true }));
  assert.ok(await reads() > initialReads, "mutation reloads desired state");
  assert.equal(await page.evaluate(() => window.__HIKYOU_SMOKE__.snapshot().modFiles.length), 1, "successful helper retained the old actual files");
  await query.fill("");
  await visible(current);
  assert.equal(await page.locator("[data-mod-entry]").count(), 1, "unresolved root is not represented as installed");
  await page.evaluate(() => window.__HIKYOU_SMOKE__.failModRequestOnce("remove"));
  await pendingRoot().getByRole("button").click();
  await visible(page.getByRole("alert").filter({ hasText: "Fixture Mod request remove refused" }));
  await visible(pendingRoot());
  await pendingRoot().getByRole("button").click();
  await count(pendingRoot(), 0);
  await visible(current);
  assert.deepEqual((await calls()).filter(call => call.command === "remove_requested_mod").at(-1).args,
    { requestedKey: "modrinth:smoke-pending", profileId: "smoke-orchard" });

  await query.fill("sodium");
  await sameName.getByTitle(en.mods.install_add, { exact: true }).click();
  await visible(pendingRoot());
  await query.fill("");
  const beforeForeign = await reads();
  await page.evaluate(() => window.__HIKYOU_SMOKE__.emitModEvent("mods://prepared", "unrelated-profile"));
  assert.equal(await reads(), beforeForeign, "foreign-profile events do not reload this panel");
  await page.evaluate(async () => {
    const fixture = window.__HIKYOU_SMOKE__;
    const state = fixture.snapshot().modRequests;
    state.preparation = { intentId: "blocked-intent", phase: "blocked", reason: "dependencies_incompatible", updatedAt: 1738540801 };
    fixture.setModRequests(state);
    await fixture.emitModEvent("mods://requests-changed");
  });
  await poll(async () => await status.getAttribute("data-phase") === "blocked", "request-changed event refreshes blocked lifecycle");
  await visible(status.getByText(en.mod_requests.current_ready, { exact: true }));
  await visible(current);
  assert.equal(await page.locator("[data-mod-entry]").count(), 1, "blocked preparation retains old files");
  assert.equal(await status.evaluate(element => element.scrollWidth <= element.clientWidth), true);
  await capture("blocked");
  await page.evaluate(async () => { const { default: i18n } = await import("/src/i18n.ts"); await i18n.changeLanguage("ja"); });
  await visible(status.getByText(ja.mod_requests.current_ready, { exact: true }));
  await capture("blocked-ja");
  await page.evaluate(async () => { const { default: i18n } = await import("/src/i18n.ts"); await i18n.changeLanguage("en"); });

  await page.evaluate(() => window.__HIKYOU_SMOKE__.failModRequestOnce("get"));
  await page.getByTitle(en.mods.refresh_btn, { exact: true }).click();
  await visible(page.getByRole("alert").filter({ hasText: "Fixture Mod request get refused" }));
  await visible(current);
  await page.getByTitle(en.mods.refresh_btn, { exact: true }).click();
  await count(page.getByRole("alert"), 0);

  // Hold an old blocked response, then publish a new ready snapshot before releasing it.
  await page.evaluate(() => window.__HIKYOU_SMOKE__.holdNextModRequestRead());
  await page.getByTitle(en.mods.refresh_btn, { exact: true }).click();
  await poll(() => page.evaluate(() => window.__HIKYOU_SMOKE__.snapshot().modReadHeld), "old request read is held");
  await page.evaluate(async () => {
    const fixture = window.__HIKYOU_SMOKE__;
    const snapshot = fixture.snapshot();
    snapshot.modRequests.pending = false;
    snapshot.modRequests.activeRevisionId = "3".repeat(64);
    snapshot.modRequests.preparation = { intentId: "ready-intent", phase: "ready", reason: null, updatedAt: 1738540802 };
    fixture.setModRequests(snapshot.modRequests, [...snapshot.modFiles, {
      filename: "arrived.jar", display_name: "New arrival", project_id: "smoke-pending", icon_url: null, size_bytes: 99,
    }]);
    await fixture.emitModEvent("mods://prepared");
  });
  await visible(page.locator('[data-mod-entry="arrived.jar"]'));
  await count(status, 0);
  await page.evaluate(() => window.__HIKYOU_SMOKE__.releaseModRequestRead());
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  await count(status, 0);
  await visible(page.locator('[data-mod-entry="arrived.jar"]'));

  await current.click();
  await visible(status.getByText(en.mod_requests.requested_disabled, { exact: true }));
  await visible(current);
  assert.equal(await page.locator('[data-mod-entry="sodium-fixture.jar.disabled"]').count(), 0, "toggle promises do not rename actual files");
  const remove = current.getByTitle(en.mods.install_delete, { exact: true });
  await current.hover();
  await remove.click();
  await remove.click();
  await count(status.locator('[data-requested-key="modrinth:smoke-sodium"]'), 0);
  await visible(current);
  assert.equal(await page.locator("[data-mod-entry]").count(), 2, "removing a root does not hide a retained dependency");

  await page.evaluate(async () => {
    const fixture = window.__HIKYOU_SMOKE__;
    const state = fixture.snapshot().modRequests;
    state.intent.requirements.push({ projectId: null, objectId: "f".repeat(64), filename: "local-request.jar", displayName: "Local request", enabled: true, iconUrl: null, origin: "imported" });
    fixture.setModRequests(state);
    await fixture.emitModEvent("mods://requests-changed");
  });
  const local = status.locator(`[data-requested-key="local:${"f".repeat(64)}"]`);
  await visible(local);
  await local.getByRole("button").click();
  await count(local, 0);
  assert.equal((await calls()).filter(call => call.command === "remove_requested_mod").at(-1).args.requestedKey, `local:${"f".repeat(64)}`);
  await capture("prepared-and-retained-dependency");
  await page.getByTitle(en.common.back, { exact: true }).click();
  await visible(page.locator(".profile-scope-overview"));
  const beforeClosed = await reads();
  await page.evaluate(() => window.__HIKYOU_SMOKE__.emitModEvent("mods://prepared"));
  assert.equal(await reads(), beforeClosed, "unmounted panel unsubscribes from preparation events");
}

async function modHistory(page, capture, smart = false) {
  const historyCommands = ["get_profile_mod_history", "get_profile_mod_revision_diff", "restore_profile_mod_revision", "resume_profile_mod_updates"];
  const calls = () => page.evaluate(() => window.__HIKYOU_SMOKE__.snapshot().calls);
  const before = await calls();
  assert.equal(before.some((call) => historyCommands.includes(call.command)), false, "history is lazy");
  const row = page.locator(".pi").filter({ hasText: "Orchard Fabric" });
  await row.click({ button: "right" });
  await page.getByRole("menuitem", { name: en.library.manage, exact: true }).click();
  await page.getByRole("option", { name: en.library.mods, exact: true }).click();
  await visible(page.getByText("Sodium", { exact: true }));
  assert.equal((await calls()).some((call) => historyCommands.includes(call.command)), false, "opening mod management does not load history");
  const trigger = page.getByRole("button", { name: en.mod_history.title, exact: true });
  await trigger.focus();
  await page.keyboard.press("Enter");
  const history = page.getByTestId("mod-history");
  const diff = history.getByTestId("mod-history-diff");
  await visible(diff);
  const profileId = smart ? "smart:latest-plus" : "smoke-orchard";
  let recorded = await calls();
  assert.deepEqual(recorded.find((call) => call.command === "get_profile_mod_history").args, { profileId });
  assert.deepEqual(recorded.find((call) => call.command === "get_profile_mod_revision_diff").args,
    { profileId, fromId: "1".repeat(64), toId: "2".repeat(64) });
  for (const value of ["0.5.11", "0.6.0", en.mod_history.enabled, en.mod_history.disabled]) {
    assert.ok((await diff.innerText()).includes(value), `diff exposes ${value}`);
  }
  assert.equal(await diff.locator('[title="mc1.20.1-0.5.11 / provider-old"]').count(), 1);
  assert.equal(await diff.locator('[title="mc1.21.1-0.6.0 / provider-current"]').count(), 1);
  for (const hash of ["a".repeat(64), "b".repeat(64)]) {
    const compactHash = diff.getByTestId("mod-object-hash").filter({ hasText: `${hash.slice(0, 12)}...${hash.slice(-8)}` });
    assert.equal(await compactHash.getAttribute("title"), hash);
    assert.equal(await compactHash.getAttribute("aria-label"), hash);
    assert.ok(await compactHash.evaluate((element) => parseFloat(getComputedStyle(element).fontSize) >= 11));
  }
  assert.ok((await diff.getByRole("region", { name: en.mod_history.added, exact: true }).innerText()).includes("lithium.jar"));
  assert.ok((await diff.getByRole("region", { name: en.mod_history.removed, exact: true }).innerText()).includes("iris.jar"));
  assert.equal(await history.getByRole("button", { name: en.mod_history.resume, exact: true }).count(), smart ? 1 : 0);
  await page.setViewportSize({ width: 750, height: 470 });
  assert.equal(await history.evaluate((element) => element.scrollWidth <= element.clientWidth), true, "history fits native window width");
  await capture("comparison");
  const from = history.getByRole("combobox", { name: en.mod_history.from, exact: true });
  await from.selectOption("2".repeat(64));
  await poll(async () => (await diff.innerText()).includes(`${en.mod_history.changed} (0)`), "same revision has no changes");
  await from.selectOption("1".repeat(64));
  await poll(async () => (await diff.innerText()).includes(`${en.mod_history.changed} (1)`), "comparison changes direction explicitly");
  const restore = history.getByRole("button", { name: en.mod_history.restore, exact: true });
  await restore.click();
  await visible(history.getByRole("alertdialog"));
  const confirmation = history.getByRole("alertdialog");
  assert.ok((await confirmation.innerText()).includes("Only the mod configuration is restored. Worlds/settings are not rolled back."));
  assert.equal(await confirmation.getByText(en.mod_history.backup_worlds, { exact: true }).count(), smart ? 1 : 0);
  if (smart) {
    await confirmation.scrollIntoViewIfNeeded();
    await capture("restore-world-warning");
    await page.evaluate(async () => { const { default: i18n } = await import("/src/i18n.ts"); await i18n.changeLanguage("ja"); });
    await visible(confirmation.getByText(ja.mod_history.backup_worlds, { exact: true }));
    assert.ok((await confirmation.innerText()).includes("復元されるのはMod構成のみです。ワールド・設定は以前の状態に戻りません。"));
    await capture("restore-world-warning-ja");
    await page.evaluate(async () => { const { default: i18n } = await import("/src/i18n.ts"); await i18n.changeLanguage("en"); });
  }
  await history.getByRole("button", { name: en.common.cancel, exact: true }).click();
  assert.equal((await calls()).some((call) => call.command === "restore_profile_mod_revision"), false, "cancel does not restore");
  await page.evaluate(() => window.__HIKYOU_SMOKE__.failHistoryOnce("restore"));
  await restore.click();
  await history.getByRole("button", { name: en.mod_history.confirm_restore, exact: true }).click();
  await visible(history.getByRole("alert").filter({ hasText: "Fixture restore refused" }));
  await capture("restore-error");
  const loads = (await calls()).filter((call) => call.command === "get_profile_mods").length;
  await restore.click();
  await history.getByRole("button", { name: en.mod_history.confirm_restore, exact: true }).click();
  await poll(async () => (await calls()).filter((call) => call.command === "get_profile_mods").length > loads, "restore reloads mods");
  await count(history.getByTestId("mod-history-revision"), 3);
  recorded = await calls();
  assert.deepEqual(recorded.filter((call) => call.command === "restore_profile_mod_revision").map((call) => call.args),
    [{ profileId, revisionId: "2".repeat(64) }, { profileId, revisionId: "2".repeat(64) }]);
  assert.equal(await history.getByRole("alert").count(), 0);
  if (smart) {
    assert.equal(await page.evaluate(() => window.__HIKYOU_SMOKE__.snapshot().pinned), true);
    await history.getByRole("button", { name: en.mod_history.resume, exact: true }).click();
    await count(history.getByTestId("mod-history-revision"), 4);
    assert.equal(await page.evaluate(() => window.__HIKYOU_SMOKE__.snapshot().pinned), false);
    assert.deepEqual((await calls()).find((call) => call.command === "resume_profile_mod_updates").args, { profileId });
  }
  await capture("restored");
  await history.getByRole("button", { name: en.common.back, exact: true }).click();
  await count(page.getByTestId("mod-history"), 0);
  await visible(trigger);
  const expectedVersion = smart ? "0.6.0" : "0.5.11";
  const expectedFilename = smart ? "sodium-0.6.0.jar" : "sodium-0.5.11.jar.disabled";
  await visible(page.getByText(`${expectedVersion} / ${expectedFilename}`, { exact: true }));
  const sodium = page.getByText("Sodium", { exact: true });
  assert.equal(await sodium.locator("../..").evaluate((element) => Number(getComputedStyle(element).opacity)), smart ? 1 : 0.4, "restored list reflects enabled state");
  await capture("installed-after-history");
  const modRow = page.locator(`[data-mod-entry="${expectedFilename}"]`);
  await sodium.click();
  await visible(page.getByRole("alert").filter({ hasText: "Fixture toggle refused" }));
  await visible(modRow);
  const remove = modRow.getByTitle(en.mods.install_delete, { exact: true });
  await modRow.hover();
  await remove.click();
  await remove.click();
  await visible(page.getByRole("alert").filter({ hasText: "Fixture remove refused" }));
  await visible(modRow);
  await capture("mutation-errors");
  // A later failed read stays visible and the refresh action can retry.
  await page.evaluate(() => window.__HIKYOU_SMOKE__.failHistoryOnce("history"));
  await trigger.click();
  await visible(page.getByTestId("mod-history").getByRole("alert"));
  await page.getByTestId("mod-history").getByRole("button", { name: en.mods.refresh_btn, exact: true }).click();
  await visible(page.getByTestId("mod-history-diff"));
  await page.keyboard.press("Escape");
  await count(page.getByTestId("mod-history"), 0);
}

async function browserRuntime(page, capture) {
  await visible(page.getByRole("status"));
  await visible(page.getByText(ja.common.desktop_required, { exact: true }));
  assert.equal(await page.evaluate(() => window.__TAURI_INTERNALS__), undefined);
  assert.equal(await page.evaluate(() => window.__HIKYOU_SMOKE__), undefined);
  await capture("desktop-required");
}

async function javaInventory(page, capture) {
  await page.getByTitle(en.nav.debug_title, { exact: true }).click();
  const inventory = page.getByTestId("java-inventory");
  await visible(inventory);
  const text = await inventory.innerText();
  for (const version of ["25.0.4+1-LTS", "25.0.3+7-LTS", "1.8.0_504-b08", "21.0.8+9-LTS"]) assert.ok(text.includes(version));
  assert.ok(text.includes(en.debug.java_selected));
  assert.ok(text.includes(en.debug.java_retained));
  assert.ok(text.includes(en.debug.java_legacy));
  for (const state of ["default", "excluded", "cleanup", "invalid"]) {
    assert.ok(await inventory.locator(`.java-status[data-state="${state}"]`).count() > 0, `${state} has an explicit status chip`);
  }
  assert.equal(await inventory.locator('.java-status svg').count(), 0, "status chips do not add decorative icons");
  assert.equal(await inventory.locator('.java-status').evaluateAll((chips) => chips.every((chip) =>
    parseFloat(getComputedStyle(chip).borderTopLeftRadius) >= chip.getBoundingClientRect().height / 2
  )), true, "Java status chips are fully rounded pills");
  assert.equal(await inventory.locator('.java-installation .java-status').evaluateAll((chips) => chips.every((chip) => {
    const row = chip.parentElement;
    return row.classList.contains("java-version") && row.lastElementChild === chip
      && parseFloat(getComputedStyle(chip).fontSize) < parseFloat(getComputedStyle(row).fontSize)
      && getComputedStyle(chip).fontWeight === "400";
  })), true, "status chips are smaller, unbolded suffixes of the version line");
  assert.equal(await inventory.locator('.java-status[data-state="cleanup"]').evaluate((element) => getComputedStyle(element).borderTopStyle), "dashed");
  assert.equal(await inventory.locator('.java-installation[data-state="excluded"] .java-status').evaluate((element) => getComputedStyle(element).borderTopStyle), "solid");
  assert.equal(await inventory.locator('.java-installation[data-state="invalid"]').evaluate((element) => getComputedStyle(element).opacity), "1", "invalid installations are not faded as cleanup");
  assert.ok(text.includes("Fixture network failure"));
  assert.ok(text.includes(en.debug.java_checking));
  assert.ok(text.includes(en.debug.java_update_deferred));
  assert.ok(text.includes("Fixture cleanup failure"));
  const groups = inventory.getByTestId("java-runtime-group");
  assert.deepEqual(await groups.evaluateAll((rows) => rows.map((row) => Number(row.dataset.javaMajor))), [25, 21, 17, 8]);
  const java25 = inventory.locator('[data-java-major="25"]');
  assert.ok((await java25.innerText()).includes("25.0.4+1-LTS"));
  assert.ok((await java25.innerText()).includes("25.0.3+7-LTS"));
  assert.equal(await java25.getByTestId("java-update-check").count(), 1, "one check summary per major, even with retained installations");
  assert.ok((await java25.innerText()).includes(en.debug.java_update_deferred));
  const java21 = await inventory.locator('[data-java-major="21"]').innerText();
  assert.ok(java21.includes("Fixture network failure") && java21.includes("Fixture cleanup failure"));
  const java17 = await inventory.locator('[data-java-major="17"]').innerText();
  assert.ok(java17.includes(en.debug.java_not_installed) && java17.includes(en.debug.java_not_checked));
  assert.ok((await inventory.locator('[data-java-major="8"]').innerText()).includes(en.debug.java_checking));
  await capture("java-desktop");
  // The native launcher is a fixed 750 x 470 window.
  await page.setViewportSize({ width: 750, height: 470 });
  assert.equal(await inventory.evaluate((element) => element.scrollWidth <= element.clientWidth), true, "inventory has no horizontal overflow");
  assert.equal(await java25.evaluate((element) => {
    const [details, check] = [...element.children].map((child) => child.getBoundingClientRect());
    return check.left >= details.right && Math.abs(details.top - check.top) < 2;
  }), true, "check metadata occupies a second column at the launcher width");
  await capture("java-compact");
  await page.setViewportSize({ width: 420, height: 700 });
  assert.equal(await inventory.evaluate((element) => element.scrollWidth <= element.clientWidth), true, "narrow inventory has no horizontal overflow");
  assert.equal(await java25.evaluate((element) => {
    const [details, check] = [...element.children].map((child) => child.getBoundingClientRect());
    return check.top >= details.bottom;
  }), true, "narrow view stacks within each version instead of overflowing");
  await page.setViewportSize({ width: 750, height: 470 });
  await inventory.getByText("Fixture network failure; previous runtime retained", { exact: true }).scrollIntoViewIfNeeded();
  await capture("java-update-status");
  await page.getByTitle(en.common.back, { exact: true }).click();
  await page.getByTitle(en.common.settings, { exact: true }).click();
  await page.locator(".settings-dropdown-wrap .log-dropdown-trigger").filter({ hasText: "English" }).click();
  await page.getByRole("button", { name: japanese, exact: true }).click();
  await visible(page.getByText(ja.settings.language_label, { exact: true }).first());
  await page.getByTitle(ja.common.back, { exact: true }).click();
  await page.getByTitle(ja.nav.debug_title, { exact: true }).click();
  await visible(page.getByText(ja.debug.section_java_inventory, { exact: true }));
  await visible(page.getByTestId("java-inventory"));
  assert.ok((await page.getByTestId("java-inventory").innerText()).includes(ja.debug.java_selected));
  await capture("java-ja");
}

async function javaSettings(page, capture) {
  await page.getByTitle(en.common.settings, { exact: true }).click();
  const settings = page.getByTestId("java-settings");
  await visible(settings);
  const checkUpdates = page.getByTestId("java-check-updates");
  await visible(checkUpdates);
  await checkUpdates.click();
  assert.equal(await checkUpdates.isDisabled(), true);
  await visible(settings.getByRole("status"));
  assert.equal(await settings.getByRole("status").innerText(), en.settings.java_check_done);
  assert.ok((await settings.innerText()).includes("25.0.5+8-LTS"));
  await checkUpdates.click();
  await visible(settings.getByRole("alert").filter({ hasText: "Fixture manual update failure" }));
  assert.equal(await checkUpdates.isEnabled(), true);
  await capture("manual-update-check");
  assert.deepEqual(await settings.getByRole("textbox").evaluateAll((inputs) => [...new Set(inputs.map((input) => input.placeholder))]), [en.settings.java_path_placeholder]);
  assert.deepEqual(await settings.getByRole("textbox").evaluateAll((inputs) => inputs.map((input) => input.value)), ["", "", "", ""], "automatic runtimes do not appear as pinned paths");
  assert.ok((await settings.innerText()).includes("Azul Zulu"));
  assert.equal((await settings.innerText()).includes("Azul Systems, Inc."), false);
  const label = (key, major) => en.settings[key].replace("{{major}}", String(major));
  const chosen = (major, path) => poll(() => page.evaluate(({ major, path }) => {
    return (window.__HIKYOU_SMOKE__.snapshot().settings.game.javaOverrides?.[major] ?? "") === path;
  }, { major, path }), `Java ${major} selection = ${path}`);
  const java21 = page.getByTestId("java-choice-21");
  assert.equal(await java21.getByRole("button").count(), 1, "only the browse action remains");
  await java21.getByRole("button", { name: label("java_browse", 21), exact: true }).click();
  await chosen(21, "/smoke/custom-21/bin/java");
  assert.ok((await java21.innerText()).includes("Fixture JRE"));
  const path = java21.getByRole("textbox", { name: label("java_location", 21), exact: true });
  await path.fill("/smoke/wrong/bin/java");
  await path.press("Enter");
  await visible(java21.getByRole("alert"));
  await chosen(21, "/smoke/custom-21/bin/java");
  await path.fill("");
  await path.press("Tab");
  await chosen(21, "");
  assert.equal(await path.inputValue(), "");
  await page.getByTestId("java-choice-8").getByRole("button", { name: label("java_browse", 8), exact: true }).click();
  await chosen(8, "");
  assert.equal(await page.getByTestId("java-choice-8").getByRole("textbox").inputValue(), "", "cancelled browse keeps Auto empty");
  const java25 = page.getByTestId("java-choice-25");
  await java25.getByRole("textbox").fill("/smoke/typed-25/bin/java");
  await java25.getByRole("button", { name: label("java_browse", 25), exact: true }).click();
  await chosen(25, "/smoke/custom-25/bin/java");
  await java25.getByRole("textbox").fill("");
  await java25.getByRole("textbox").press("Enter");
  await chosen(25, "");
  assert.ok((await java25.innerText()).includes(en.settings.java_automatic));
  assert.ok((await java25.innerText()).includes("25.0.5+8-LTS"), "Auto exposes the manually updated version outside the input");
  const java17 = page.getByTestId("java-choice-17");
  await java17.getByRole("textbox").fill("/smoke/custom-17/bin/java");
  await java17.getByRole("textbox").press("Tab");
  await chosen(17, "/smoke/custom-17/bin/java");
  await page.setViewportSize({ width: 750, height: 470 });
  await settings.scrollIntoViewIfNeeded();
  assert.equal(await settings.evaluate((element) => element.scrollWidth <= element.clientWidth), true);
  await capture("java-selections");
  await page.locator(".settings-dropdown-wrap .log-dropdown-trigger").filter({ hasText: "English" }).click();
  await page.getByRole("button", { name: japanese, exact: true }).click();
  await visible(page.getByText(ja.settings.java_group, { exact: true }));
  await capture("java-selections-ja");
  await java17.getByRole("textbox").fill("/smoke/edited-17/bin/java");
  await page.getByTitle(ja.common.back, { exact: true }).click();
  await chosen(17, "/smoke/edited-17/bin/java");
  await page.getByTitle(ja.common.settings, { exact: true }).click();
  assert.equal(await page.getByTestId("java-choice-17").getByRole("textbox").inputValue(), "/smoke/edited-17/bin/java", "blur saves even when leaving Settings");
}

async function javaSettingsLegacy(page, capture) {
  await page.getByTitle(en.common.settings, { exact: true }).click();
  const row = page.getByTestId("java-choice-21");
  await visible(row);
  assert.equal(await row.getByRole("textbox").inputValue(), "/smoke/legacy/bin/java");
  await row.getByRole("textbox").fill("");
  await row.getByRole("textbox").press("Tab");
  await poll(() => page.evaluate(() => window.__HIKYOU_SMOKE__.snapshot().settings.game.javaOverrides?.[21] === ""), "explicit Auto is persisted");
  assert.equal(await row.getByRole("textbox").inputValue(), "");
  assert.ok((await row.innerText()).includes(en.settings.java_automatic));
  assert.equal(await page.getByTestId("java-choice-25").getByRole("textbox").inputValue(), "/smoke/legacy/bin/java", "restoring one major does not change others");
  await page.getByTitle(en.common.back, { exact: true }).click();
  await page.getByTitle(en.common.settings, { exact: true }).click();
  assert.equal(await page.getByTestId("java-choice-21").getByRole("textbox").inputValue(), "", "Auto survives reopening Settings despite the legacy fallback");
  await capture("java-legacy-auto");
}

export async function installIsolation(context, baseUrl, evidence, { injectFixture = true } = {}) {
  const origin = new URL(baseUrl).origin;
  await context.route("**/*", async (route) => {
    try {
      const url = new URL(route.request().url());
      if (url.origin !== origin) {
        evidence.blockedRequests.push({ url: url.href, kind: "http" });
        await route.abort("blockedbyclient");
        return;
      }
      if (injectFixture && url.pathname === "/src/main.ts") {
        const response = await route.fetch({ timeout: 15000, maxRedirects: 0 });
        assert.equal(response.status(), 200, "production main.ts must be served by Vite");
        const original = await response.text();
        const headers = { ...response.headers(), "content-type": "text/javascript; charset=utf-8" };
        delete headers["content-length"]; delete headers["content-encoding"];
        await route.fulfill({ response, headers, body: `import '/tests/frontend/fixture.ts';\n${original}` });
        evidence.entryInjections++;
        return;
      }
      await route.continue();
    } catch (error) {
      if (!evidence.closing) evidence.routeErrors.push({ url: route.request().url(), error: error.message });
      await route.abort("failed").catch(() => {});
    }
  });
  if (!context.routeWebSocket) throw new Error("This smoke runner requires Playwright WebSocket routing support");
  await context.routeWebSocket("**/*", (socket) => {
    const url = new URL(socket.url());
    const expected = new URL(origin);
    expected.protocol = expected.protocol === "https:" ? "wss:" : "ws:";
    if (url.origin === expected.origin) socket.connectToServer();
    else {
      evidence.blockedRequests.push({ url: url.href, kind: "websocket" });
      socket.close({ code: 1008, reason: "External network is disabled in frontend smoke tests" });
    }
  });
}

async function advancedUnlock(page, capture) {
  await page.setViewportSize({ width: 750, height: 470 });
  const trigger = page.locator("[data-advanced-trigger]");
  const progress = page.locator("[data-advanced-progress]");
  const tap = async count => { for (let index = 0; index < count; index++) await trigger.click(); };
  await tap(2);
  await count(progress, 0);
  await tap(1);
  await visible(progress);
  assert.match(await progress.innerText(), /^4 more clicks/);
  await capture("remaining-clicks");
  await count(page.getByRole("dialog", { name: en.advanced_confirm.title, exact: true }), 0);
  await poll(async () => await progress.count() === 0, "unlock count expires after a pause");
  await tap(6);
  assert.match(await progress.innerText(), /^1 more click /);
  await tap(1);
  const dialog = page.getByRole("dialog", { name: en.advanced_confirm.title, exact: true });
  await visible(dialog);
  assert.equal(await page.evaluate(() => window.__HIKYOU_SMOKE__.snapshot().settings.advanced.enabled), false);
  await dialog.getByRole("button", { name: en.common.enable, exact: true }).click();
  await poll(() => page.evaluate(() => window.__HIKYOU_SMOKE__.snapshot().settings.advanced.enabled), "advanced mode enabled only after confirmation");
  await tap(3);
  await count(progress, 0);
}

export async function runFrontendSmoke({ chromium, url, outputDir, browserChannel, scenarioIds }) {
  await mkdir(outputDir, { recursive: true });
  const browser = await chromium.launch({ headless: true, channel: browserChannel || undefined });
  const results = [];
  try {
    for (const [id, scenario, native = true] of [
      ["browser-runtime", browserRuntime, false],
      ["main-keyboard", mainKeyboard],
      ["advanced-unlock", advancedUnlock],
      ["search-selection", searchSelection],
      ["modpack-aliases", modpackAliases],
      ["settings-locale", settingsLocale],
      ["profile-library", profileLibrary],
      ["mod-history", modHistory],
      ["mod-requests", pendingModRequests],
      ["mod-history-smart", (page, capture) => modHistory(page, capture, true)],
      ["java-inventory", javaInventory],
      ["java-settings", javaSettings],
      ["java-settings-legacy", javaSettingsLegacy],
    ].filter(([id]) => !scenarioIds || scenarioIds.includes(id))) {
      const result = {
        id, label: "main", status: "running", console: [], pageErrors: [], blockedRequests: [],
        requestFailures: [], routeErrors: [], entryInjections: 0, screenshots: [], closing: false,
      };
      results.push(result);
      console.log(`Frontend smoke: running ${id}`);
      const context = await browser.newContext({ viewport: { width: 1040, height: 760 }, locale: "en-US", timezoneId: "UTC", serviceWorkers: "block", colorScheme: "dark", acceptDownloads: false });
      context.setDefaultTimeout(12000);
      let page;
      const capture = async (name, { animations = "disabled" } = {}) => {
        const filename = `${id}-${name}.png`;
        await page.screenshot({ path: join(outputDir, filename), fullPage: true, animations, timeout: 12000 });
        result.screenshots.push(filename);
      };
      try {
        await installIsolation(context, url, result, { injectFixture: native });
        page = await context.newPage();
        page.on("console", (message) => result.console.push({ type: message.type(), text: message.text(), location: message.location() }));
        page.on("pageerror", (error) => result.pageErrors.push(error.stack ?? error.message));
        page.on("requestfailed", (request) => result.requestFailures.push({ url: request.url(), error: request.failure()?.errorText }));
        const target = new URL(url);
        target.searchParams.set("locale", "en");
        if (id === "java-inventory") target.searchParams.set("smoke_debug", "1");
        if (id === "java-settings-legacy") target.searchParams.set("smoke_legacy_java", "1");
        if (id === "mod-history-smart") target.searchParams.set("smoke_history_smart", "1");
        if (id === "mod-requests") target.searchParams.set("smoke_mod_requests", "1");
        await page.goto(target.href, { waitUntil: "domcontentloaded", timeout: 30000 });
        if (native) {
          await page.waitForFunction(() => Boolean(window.__HIKYOU_SMOKE__));
          assert.equal(await page.evaluate(() => window.__HIKYOU_SMOKE__.label), "main");
          assert.equal(await page.evaluate(() => window.__HIKYOU_SMOKE__.checkEvents()), true, "official Tauri event mocks must dispatch events");
          await visible(mainSearch(page));
        }
        await scenario(page, capture);
        await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        if (native) {
          result.fixture = await page.evaluate(() => window.__HIKYOU_SMOKE__.snapshot());
          assert.ok(result.entryInjections > 0, "main.ts fixture injection was used");
          assert.deepEqual(result.fixture.unexpected, [], "all IPC commands must be explicitly mocked");
        } else assert.equal(result.entryInjections, 0, "ordinary browser uses no Tauri mocks");
        assert.deepEqual(result.blockedRequests, [], "frontend attempted external network access");
        assert.deepEqual(result.pageErrors, [], "browser page errors");
        assert.deepEqual(result.console.filter((item) => item.type === "error"), [], "browser console errors");
        assert.deepEqual(result.requestFailures, [], "browser request failures");
        assert.deepEqual(result.routeErrors, [], "browser interception errors");
        assert.equal(await page.getByText(en.error_boundary.title, { exact: true }).count(), 0, "application error boundary is not rendered");
        assert.equal(await page.getByText(ja.error_boundary.title, { exact: true }).count(), 0, "localized application error boundary is not rendered");
        result.status = "passed";
      } catch (error) {
        result.status = "failed"; result.error = error.stack ?? String(error);
        if (page) {
          await capture("failure").catch(() => {});
          result.fixture = await page.evaluate(() => window.__HIKYOU_SMOKE__?.snapshot()).catch(() => null);
          result.bodyText = await page.locator("body").innerText().then((text) => text.slice(0, 4000)).catch(() => "");
        }
      } finally {
        result.closing = true;
        await context.unrouteAll({ behavior: "ignoreErrors" });
        await context.close();
        delete result.closing;
      }
    }
  } finally { await browser.close(); }
  return {
    status: results.every((result) => result.status === "passed") ? "passed" : "failed",
    tests: results,
    suite: fileURLToPath(import.meta.url),
  };
}
