import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";
import { assertPrivateArtifact, privateBuildEnv, projectRoot } from "./build-privacy.mjs";
import { acquireSession, runChild, runDev } from "./tauri-session.mjs";

const require = createRequire(import.meta.url);

export async function runTauri(args, { run = runChild, dev = runDev, acquire = acquireSession } = {}) {
  const targetIndex = args.findIndex((arg) => arg === "--target" || arg === "-t");
  const target = args.find((arg) => arg.startsWith("--target="))?.slice(9) ||
    (targetIndex < 0 ? process.env.CARGO_BUILD_TARGET : args[targetIndex + 1]);
  const env = privateBuildEnv();
  env.HIKYOU_BUILD_TARGET = target || "";
  env.HIKYOU_BUILD_PROFILE = args.includes("--debug") || args.includes("-d") ? "debug" : "release";
  if (env.CARGO_TARGET_DIR) env.CARGO_TARGET_DIR = resolve(env.CARGO_TARGET_DIR);
  const help = args.some((arg) => ["--help", "-h", "--version", "-V"].includes(arg));
  const options = { cwd: projectRoot, env, stdio: "inherit" };
  const controller = new AbortController();
  const abort = () => controller.abort();
  const managed = ["dev", "build"].includes(args[0]) && !help;
  const release = managed ? await acquire() : undefined;
  for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, abort);
  try {
    if (controller.signal.aborted) return 130;
    const cliArgs = [require.resolve("@tauri-apps/cli/tauri.js"), ...args];
    const status = args[0] === "dev" && !help
      ? await dev(cliArgs, options, controller.signal, run)
      : await run(process.execPath, cliArgs, options, controller.signal);
    if (status !== 0) return status;

    if (args[0] === "build" && !help) {
      const profile = args.includes("--debug") || args.includes("-d") ? "debug" : "release";
      const output = resolve(process.env.CARGO_TARGET_DIR || resolve(projectRoot, "src-tauri/target"));
      const binary = resolve(output, ...(target ? [target] : []), profile,
        process.platform === "win32" ? "hikyou-launcher.exe" : "hikyou-launcher");
      if (!existsSync(binary)) throw new Error("Cannot verify the built application path.");
      assertPrivateArtifact(binary);
      console.log("Application build-path privacy check passed.");
    }
    return 0;
  } finally {
    for (const signal of ["SIGINT", "SIGTERM"]) process.off(signal, abort);
    await release?.();
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try { process.exitCode = await runTauri(process.argv.slice(2)); }
  catch (error) { console.error(`Tauri stopped: ${error.message}`); process.exitCode = 1; }
}
