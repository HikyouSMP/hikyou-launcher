import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { resolve, join } from "node:path";
import { pathToFileURL } from "node:url";
import { createServer } from "vite";

const require = createRequire(process.env.HIKYOU_TEST_RUNTIME_MODULES ? join(resolve(process.env.HIKYOU_TEST_RUNTIME_MODULES), "__ui_runtime__.cjs") : import.meta.url);
const runtime = await import(pathToFileURL(require.resolve("playwright")).href);
const { chromium } = runtime.chromium ? runtime : runtime.default;
console.log("UI fixes: starting local fixture server");
const server = process.env.HIKYOU_TEST_URL ? null : await createServer({ server: { host: "127.0.0.1", port: 0, strictPort: false } });
if (server) await server.listen();
const address = server?.httpServer.address();
const origin = process.env.HIKYOU_TEST_URL ?? `http://127.0.0.1:${address.port}`;
const output = resolve(".local/ui-fixes");
await mkdir(output, { recursive: true });
console.log(`UI fixes: launching browser for ${origin}`);
const browser = await chromium.launch({ headless: true, channel: process.env.HIKYOU_TEST_BROWSER_CHANNEL || undefined, args: ["--no-proxy-server"] });
const context = await browser.newContext({ viewport: { width: 750, height: 470 }, serviceWorkers: "block" });
const errors = [];
const external = [];
let entry = "modListScroll.entry.ts";
await context.route("**/*", async route => {
  const url = new URL(route.request().url());
  if (url.origin !== origin) { external.push(url.href); await route.abort(); return; }
  if (url.pathname === "/src/main.ts") { await route.fulfill({ contentType: "text/javascript", body: `import '/tests/frontend/${entry}';` }); return; }
  await route.continue();
});
await context.routeWebSocket("**/*", socket => {
  if (new URL(socket.url()).origin === origin.replace("http:", "ws:")) socket.connectToServer();
  else { external.push(socket.url()); socket.close(); }
});
const page = await context.newPage();
page.setDefaultTimeout(90000);
page.on("pageerror", error => errors.push(error.message));
const settle = () => page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
try {
  console.log("UI fixes: checking keyboard and icon fixture");
  await page.goto(origin, { waitUntil: "domcontentloaded", timeout: 90000 });
  const rows = page.locator("[data-mod-entry]");
  await rows.first().waitFor();
  await page.waitForFunction(() => document.images.length === 20 && [...document.images].every(image => image.complete && image.naturalWidth > 0));
  assert.equal(await page.locator("[data-saves]").getAttribute("data-saves"), "0", "icon enrichment must not save Mod policy");
  assert.equal(await page.locator("[data-metadata-requests]").getAttribute("data-metadata-requests"), "1");
  const input = page.locator("input").first();
  await input.focus();
  await input.press("Delete");
  assert.equal(await rows.count(), 20);
  assert.equal(await rows.first().locator(".text-danger").count(), 1);
  await input.dispatchEvent("keydown", { key: "Delete", repeat: true });
  await input.dispatchEvent("keydown", { key: "Enter", repeat: true });
  assert.equal(await rows.count(), 20);
  await input.press("Escape");
  assert.equal(await rows.first().locator(".text-danger").count(), 0);
  await input.press("Backspace");
  await input.press("Enter");
  assert.equal(await rows.count(), 19);
  await input.press("Delete");
  await input.press("Backspace");
  assert.equal(await rows.count(), 18, "both new keys share the same confirmation path");
  await input.fill("abc");
  await input.evaluate(node => node.setSelectionRange(1, 1));
  await input.press("Backspace");
  assert.equal(await input.inputValue(), "bc");
  assert.equal(await page.locator("[data-saves]").getAttribute("data-saves"), "2", "editing query must not remove a Mod");
  await input.fill("");
  await settle();
  for (const flags of [{ isComposing: true }, { ctrlKey: true }, { metaKey: true }, { shiftKey: true }, { repeat: true }]) {
    await input.dispatchEvent("keydown", { key: "Delete", ...flags });
    assert.equal(await rows.first().locator(".text-danger").count(), 0);
  }
  await input.fill(" ");
  await input.press("Delete");
  assert.equal(await rows.first().locator(".text-danger").count(), 0, "whitespace is still editable query text");
  await input.fill("");
  await page.evaluate(() => {
    const root = document.querySelector("[data-closed]").firstElementChild;
    for (const tag of ["textarea", "div", "button"]) {
      const node = document.createElement(tag);
      node.dataset.editTest = tag;
      if (tag === "div") node.contentEditable = "true";
      root.append(node);
    }
  });
  await input.press("Delete");
  for (const tag of ["textarea", "div", "button"]) {
    const control = page.locator(`[data-edit-test="${tag}"]`);
    await control.dispatchEvent("keydown", { key: "Delete" });
    await control.dispatchEvent("keydown", { key: "Enter" });
  }
  assert.equal(await rows.count(), 18, "other controls cannot confirm an armed removal");
  await input.press("Escape");
  await page.evaluate(() => document.querySelectorAll("[data-edit-test]").forEach(node => node.remove()));
  await page.evaluate(() => { const dialog = document.createElement("dialog"); dialog.dataset.testModal = "true"; document.body.append(dialog); dialog.showModal(); });
  await input.dispatchEvent("keydown", { key: "Delete" });
  assert.equal(await rows.first().locator(".text-danger").count(), 0);
  await page.evaluate(() => { const dialog = document.querySelector("[data-test-modal]"); dialog.close(); dialog.remove(); });
  await input.focus();
  await input.press("Alt+ArrowLeft");
  await input.press("Enter");
  assert.equal(await rows.count(), 17, "existing Alt+Left path remains functional");
  await page.screenshot({ path: join(output, "keyboard-icons.png") });

  entry = "uiControls.entry.ts";
  console.log("UI fixes: measuring button contrast");
  await page.goto(origin, { waitUntil: "domcontentloaded", timeout: 90000 });
  await page.getByRole("button", { name: "Apply", exact: true }).waitFor();
  const measurements = [];
  for (const underlay of ["#000000", "#262624", "#ffffff"]) {
    await page.locator("[data-underlay]").evaluate((node, value) => { node.style.background = value; }, underlay);
    await settle();
    const png = await page.screenshot();
    const values = await page.evaluate(async imageData => {
      const image = new Image(); image.src = imageData; await image.decode();
      const canvas = document.createElement("canvas"); canvas.width = image.width; canvas.height = image.height;
      const ctx = canvas.getContext("2d"); ctx.drawImage(image, 0, 0);
      const luminance = rgb => rgb.map(value => { const s = value / 255; return s <= .04045 ? s / 12.92 : ((s + .055) / 1.055) ** 2.4; }).reduce((sum, value, index) => sum + value * [.2126, .7152, .0722][index], 0);
      return [...document.querySelectorAll("button:not(:disabled)")].map(node => {
        const style = getComputedStyle(node); const box = node.getBoundingClientRect();
        const background = [...ctx.getImageData(Math.floor(box.left + 3), Math.floor(box.top + box.height / 2), 1, 1).data].slice(0, 3);
        const foreground = style.color.match(/[\d.]+/g).slice(0, 3).map(Number);
        const a = luminance(foreground), b = luminance(background);
        return { label: node.textContent.trim() || node.title, foreground, background, contrast: (Math.max(a, b) + .05) / (Math.min(a, b) + .05) };
      });
    }, `data:image/png;base64,${png.toString("base64")}`);
    measurements.push({ underlay, controls: values });
    await page.screenshot({ path: join(output, `contrast-${underlay.slice(1)}.png`) });
  }
  await writeFile(join(output, "contrast.json"), JSON.stringify(measurements, null, 2));
  for (const surface of measurements) for (const control of surface.controls) assert.ok(control.contrast >= 4.5, `${surface.underlay} ${control.label}: ${control.contrast.toFixed(2)}:1`);
  const disabled = await page.locator("button:disabled").evaluateAll(nodes => nodes.map(node => Number(getComputedStyle(node).opacity)));
  assert.ok(disabled.every(opacity => opacity <= .4));
  assert.deepEqual(errors, []);
  assert.deepEqual(external, []);
  console.log("PASS: removal/editing/IME/held-key/modal boundaries, metadata-only icon loading, zero policy saves, enabled contrast >=4.5:1, disabled distinction, no external requests.");
} finally { await context.close(); await browser.close(); await server?.close(); }
