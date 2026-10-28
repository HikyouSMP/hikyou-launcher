import { mkdir, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { runFrontendSmoke } from "../tests/frontend/smoke.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outputDir = join(root, ".local", "frontend-smoke");
const url = process.env.HIKYOU_TEST_URL ?? "http://127.0.0.1:1420";
const browserChannel = process.env.HIKYOU_TEST_BROWSER_CHANNEL;
const startedAt = new Date().toISOString();
let summary;

async function loadPlaywright() {
  const modules = process.env.HIKYOU_TEST_RUNTIME_MODULES;
  let imported;
  if (!modules) imported = await import("playwright");
  else {
    const require = createRequire(join(resolve(modules), "__hikyou_smoke_runtime__.cjs"));
    imported = await import(pathToFileURL(require.resolve("playwright")).href);
  }
  const runtime = imported.chromium ? imported : imported.default;
  if (!runtime?.chromium) throw new Error("Resolved Playwright module does not expose chromium");
  return runtime;
}

try {
  const target = new URL(url);
  if (!["http:", "https:"].includes(target.protocol)) throw new Error("HIKYOU_TEST_URL must be an HTTP(S) dev server URL");
  let ready = false;
  let readinessError;
  try {
    const response = await fetch(target, { signal: AbortSignal.timeout(5000), redirect: "error" });
    ready = response.ok;
    if (!ready) readinessError = `HTTP ${response.status}`;
    await response.body?.cancel();
  } catch (error) { readinessError = [error.message, error.cause?.code].filter(Boolean).join(": "); }
  if (!ready) {
    summary = { status: "skipped", reason: `Dev server is not ready: ${readinessError}`, tests: [] };
    process.exitCode = 2;
  } else {
    let playwright;
    try { playwright = await loadPlaywright(); }
    catch (error) { throw new Error("Cannot resolve Playwright. Set HIKYOU_TEST_RUNTIME_MODULES to the bundled node_modules directory, or provide a normally resolvable playwright installation.", { cause: error }); }
    summary = await runFrontendSmoke({ chromium: playwright.chromium, url: target.href, outputDir, browserChannel });
    if (summary.status !== "passed") process.exitCode = 1;
  }
} catch (error) {
  summary = { status: "failed", error: error.stack ?? String(error), cause: error.cause?.message, tests: [] };
  process.exitCode = 1;
} finally {
  await mkdir(outputDir, { recursive: true });
  summary = { ...summary, url, browserChannel: browserChannel || "default", startedAt, finishedAt: new Date().toISOString() };
  await writeFile(join(outputDir, "summary.json"), `${JSON.stringify(summary, null, 2)}\n`);
  console.log(`Frontend smoke: ${summary.status}`);
  for (const result of summary.tests) console.log(`  ${result.status}: ${result.id}${result.error ? `\n    ${result.error.split("\n")[0]}` : ""}`);
  if (summary.reason ?? summary.error) console.log(summary.reason ?? summary.error);
  console.log(`Artifacts: ${outputDir}`);
  if (summary.status === "skipped") console.log("After starting the Vite dev server, run: node scripts/frontend-smoke.mjs");
}
