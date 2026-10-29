import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { execFileSync } from "node:child_process";
import { resolve, join } from "node:path";
import { pathToFileURL } from "node:url";
import { createServer } from "vite";

const require = createRequire(process.env.HIKYOU_TEST_RUNTIME_MODULES
  ? join(resolve(process.env.HIKYOU_TEST_RUNTIME_MODULES), "__mod_list_runtime__.cjs") : import.meta.url);
const imported = await import(pathToFileURL(require.resolve("playwright")).href);
const { chromium } = imported.chromium ? imported : imported.default;
console.log("Mod list: starting local server");
const baseline = process.env.HIKYOU_SCROLL_BASELINE_REF;
const original = resolve("src/components/ModListView.svelte");
const server = process.env.HIKYOU_TEST_URL ? null : await createServer({
  plugins: baseline ? [{ name: "baseline-mod-list", enforce: "pre", load(id) {
    if (resolve(id.split("?")[0]) === original) return execFileSync("git", ["show", `${baseline}:src/components/ModListView.svelte`], { encoding: "utf8" });
  } }] : [],
  server: { host: "127.0.0.1", port: 0 },
});
if (server) await server.listen();
console.log("Mod list: launching browser");
const address = server?.httpServer?.address();
const url = process.env.HIKYOU_TEST_URL ?? `http://127.0.0.1:${address.port}`;
const output = resolve(".local/mod-list-scroll");
await mkdir(output, { recursive: true });
const browser = await chromium.launch({ headless: true, channel: process.env.HIKYOU_TEST_BROWSER_CHANNEL || undefined });
const context = await browser.newContext({ viewport: { width: 750, height: 470 }, serviceWorkers: "block" });
const page = await context.newPage();
page.setDefaultTimeout(90000);
console.log("Mod list: loading fixture");
const errors = [];
page.on("pageerror", (error) => errors.push(error.message));
try {
  await context.route("**/*", async (route) => {
    const target = new URL(route.request().url());
    if (target.origin !== new URL(url).origin) return route.abort();
    if (target.pathname === "/src/main.ts") return route.fulfill({ contentType: "text/javascript", body: "import '/tests/frontend/modListScroll.entry.ts';" });
    await route.continue();
  });
  await page.goto(url, { waitUntil: "domcontentloaded", timeout: 90000 });
  const rows = page.locator("[data-mod-entry]");
  await rows.first().waitFor();
  assert.equal(await rows.count(), 20);
  const list = rows.first().locator("..");
  await page.waitForFunction(() => [...document.images].every((image) => image.complete && image.naturalWidth > 0));
  const box = await rows.nth(2).boundingBox();
  await page.mouse.move(box.x + 120, box.y + 20);
  await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(resolve)));
  const measurement = await list.evaluate(async (element) => {
    const mutations = [];
    const frames = [];
    const observer = new MutationObserver((records) => mutations.push(...records));
    observer.observe(element, { subtree: true, attributes: true, attributeFilter: ["style"] });
    let events = 0;
    const onScroll = () => events++;
    element.addEventListener("scroll", onScroll);
    let last = performance.now();
    for (let index = 0; index < 48; index++) {
      await new Promise((resolve) => requestAnimationFrame(resolve));
      const now = performance.now(); frames.push(now - last); last = now;
      element.scrollTop += index < 24 ? 12 : -12;
    }
    await new Promise((resolve) => setTimeout(resolve, 220));
    observer.disconnect(); element.removeEventListener("scroll", onScroll);
    return { scrollEvents: events, listStyleMutations: mutations.filter((record) => record.target === element).length,
      totalStyleMutations: mutations.length, maxFrameMs: Math.max(...frames), iconCount: element.querySelectorAll("img").length,
      backdropFilters: [...element.querySelectorAll("*")].filter((node) => getComputedStyle(node).backdropFilter !== "none").length };
  });
  await page.screenshot({ path: join(output, "recommendations.png") });
  assert.ok(measurement.scrollEvents >= 30, "exercise repeated real scroll events");
  assert.equal(measurement.iconCount, 20);
  assert.equal(measurement.backdropFilters, 0, "no per-row backdrop filters");
  if (!process.env.HIKYOU_SCROLL_BASELINE) assert.equal(measurement.listStyleMutations, 0, "scrolling must not rewrite list-wide hit testing");
  await page.mouse.move(box.x + 125, box.y + 20);
  await rows.nth(2).click();
  assert.equal(await rows.nth(2).locator("img").evaluate((image) => image.style.opacity), "0.35");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  assert.equal(await rows.nth(1).locator("img").evaluate((image) => image.style.opacity), "0.35");
  await page.keyboard.press("Alt+ArrowLeft");
  await page.keyboard.press("Enter");
  assert.equal(await rows.count(), 19, "keyboard delete still works");
  await page.locator("input").fill("fixture");
  await page.getByText("Search result", { exact: true }).waitFor();
  await page.keyboard.press("Enter");
  await page.keyboard.press("Escape");
  assert.equal(await rows.count(), 20, "search install still works");
  await page.keyboard.press("Escape");
  assert.equal(await page.locator("[data-closed]").getAttribute("data-closed"), "true");
  assert.deepEqual(errors, []);
  await writeFile(join(output, process.env.HIKYOU_SCROLL_BASELINE ? "baseline.json" : "result.json"), `${JSON.stringify(measurement, null, 2)}\n`);
  console.log(JSON.stringify(measurement, null, 2));
} finally {
  await context.close();
  await browser.close();
  await server?.close();
}
