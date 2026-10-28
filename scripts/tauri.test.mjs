import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { projectRoot } from "./build-privacy.mjs";
import { runTauri } from "./tauri.mjs";

const acquire = async () => async () => {};
const dev = (args, options, signal, run) => run(process.execPath, args, options, signal);

test("dev starts Tauri without a sidecar build and preserves target arguments", async () => {
  for (const args of [["dev"], ["dev", "-t", "aarch64-pc-windows-msvc"], ["dev", "--target=x86_64-pc-windows-msvc"]]) {
    let calls = 0;
    expect(await runTauri(args, { acquire, dev, run: async (_, commandArgs) => {
      calls++;
      expect(commandArgs.slice(1)).toEqual(args);
      return 0;
    } })).toBe(0);
    expect(calls).toBe(1);
  }
});

test("CLI failure releases the owned session", async () => {
  let released = 0;
  expect(await runTauri(["dev"], {
    acquire: async () => async () => { released++; }, dev,
    run: async () => 7,
  })).toBe(7);
  expect(released).toBe(1);
});

test("help bypasses the session and preserves CLI exit status", async () => {
  for (const args of [["dev", "--help"], ["info"], ["build", "--help"]]) {
    expect(await runTauri(args, { acquire: () => { throw new Error("not a build"); },
      run: async (_, commandArgs) => {
        expect(commandArgs.slice(1)).toEqual(args);
        return args.includes("--help") ? 0 : 3;
      },
    })).toBe(args.includes("--help") ? 0 : 3);
  }
});

test("development and packaging do not require or distribute .NET", () => {
  const config = JSON.parse(readFileSync(resolve(projectRoot, "src-tauri/tauri.conf.json"), "utf8"));
  expect(config.build.beforeDevCommand).toBe("bun run dev");
  expect(config.build.beforeBuildCommand).toBe("bun run build");
  expect(config.build.beforeBundleCommand).toBe("bun scripts/check-bundle-inputs.mjs");
  const windows = JSON.parse(readFileSync(resolve(projectRoot, "src-tauri/tauri.windows.conf.json"), "utf8"));
  expect(windows.bundle.externalBin).toEqual([]);
  expect(windows.bundle.resources).toEqual([]);
  for (const workflow of ["check.yml", "release.yml"]) {
    const text = readFileSync(resolve(projectRoot, ".github/workflows", workflow), "utf8");
    expect(text).not.toContain("setup-dotnet");
    expect(text).not.toContain("prepare:auth-broker");
  }
});
