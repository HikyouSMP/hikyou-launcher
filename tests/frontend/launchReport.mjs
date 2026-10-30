import assert from "node:assert/strict";

export async function launchReport(page, capture) {
  await page.evaluate(async () => {
    const fixture = await import("/tests/frontend/launchReportFixture.ts");
    fixture.mountLaunchError();
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: {
      writeText: async () => { throw new Error("clipboard refused"); },
    } });
  });
  const affordance = page.getByRole("button", { name: "Report this error...", exact: true });
  await affordance.focus();
  assert.equal(await affordance.evaluate(node => document.activeElement === node), true);
  await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog", { name: "Review Launch Error Report" });
  await dialog.waitFor({ state: "visible" });
  const preview = dialog.getByRole("textbox", { name: "Report preview" });
  await preview.waitFor({ state: "visible" });
  const open = dialog.getByRole("button", { name: "Open GitHub", exact: true });
  const copy = dialog.getByRole("button", { name: "Copy report", exact: true });
  assert.equal(await open.isDisabled(), true);
  assert.equal(await copy.isDisabled(), true);
  assert.equal(await page.evaluate(() => window.__HIKYOU_SMOKE__.calls.some(call => call.command === "open_launch_error_report")), false);
  assert.match(await preview.inputValue(), /search states\/projects/);
  for (const key of ["Tab", "Tab", "Tab", "Shift+Tab", "Shift+Tab", "Shift+Tab"]) {
    await page.keyboard.press(key);
    assert.equal(await dialog.evaluate(node => node.contains(document.activeElement)), true);
  }
  await dialog.getByRole("checkbox").check();
  await open.click();
  await dialog.getByRole("alert").filter({ hasText: "GitHub could not be opened" }).waitFor();
  await copy.click();
  await dialog.getByRole("alert").filter({ hasText: "Copy failed" }).waitFor();
  await capture("copy-failure");
  for (const viewport of [{ width: 750, height: 470 }, { width: 360, height: 640 }]) {
    await page.setViewportSize(viewport);
    assert.equal(await dialog.evaluate(node => {
      const card = node.querySelector("section").getBoundingClientRect();
      return card.left >= 0 && card.right <= innerWidth && card.top >= 0 && card.bottom <= innerHeight;
    }), true);
    await capture(`preview-${viewport.width}`);
  }
  await page.keyboard.press("Escape");
  await dialog.waitFor({ state: "hidden" });
  assert.equal(await affordance.evaluate(node => document.activeElement === node), true);
  const calls = await page.evaluate(() => window.__HIKYOU_SMOKE__.calls.filter(call => /launch_error_report/.test(call.command)));
  assert.deepEqual(calls.map(call => call.command), ["prepare_launch_error_report", "open_launch_error_report"]);
  assert.equal(calls[1].args.reviewed, true);
  assert.equal("error" in calls[1].args, false);
}
