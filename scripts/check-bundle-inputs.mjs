import { readdirSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { projectRoot } from "./build-privacy.mjs";

export function checkWindowsBundleInputs(directory) {
  // WiX automatically includes adjacent DLLs, even when bundle.resources is
  // empty. This application has no separately distributed Windows DLLs.
  const unexpected = readdirSync(directory, { withFileTypes: true })
    .filter(entry => entry.name.toLowerCase().endsWith(".dll"));
  if (unexpected.length) {
    throw new Error("Unexpected DLLs remain in the Windows build directory. Clean that generated build output before packaging; stale files must not enter the installer.");
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const target = process.env.HIKYOU_BUILD_TARGET || process.env.CARGO_BUILD_TARGET;
  if (target ? target.includes("windows") : process.platform === "win32") {
    const root = resolve(process.env.CARGO_TARGET_DIR || resolve(projectRoot, "src-tauri/target"));
    checkWindowsBundleInputs(resolve(root, ...(target ? [target] : []), process.env.HIKYOU_BUILD_PROFILE || "release"));
    console.log("Windows bundle input check passed (no adjacent DLLs).");
  }
}
