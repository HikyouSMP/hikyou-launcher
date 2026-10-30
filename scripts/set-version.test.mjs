import { expect, test } from "bun:test";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve, sep } from "node:path";
import { spawnSync } from "node:child_process";

function workspace(run) {
  const root = mkdtempSync(join(tmpdir(), "hikyou-version-"));
  if (!resolve(root).startsWith(resolve(tmpdir()) + sep)) throw new Error("Unexpected fixture root");
  try {
    mkdirSync(join(root, "scripts"));
    mkdirSync(join(root, "src-tauri"));
    for (const name of ["set-version.mjs", "versioning.mjs"]) {
      copyFileSync(join(import.meta.dirname, name), join(root, "scripts", name));
    }
    writeFileSync(join(root, "package.json"), JSON.stringify({ name: "fixture", version: "26.1.0-beta.7", type: "module" }));
    writeFileSync(join(root, "src-tauri/Cargo.toml"), '[package]\nname = "fixture"\nversion = "26.1.0-beta.7"\n\n[dependencies]\nexample = { version = "1" }\n');
    return run(root);
  } finally {
    rmSync(root, { recursive: true });
  }
}

function runVersion(root, config) {
  writeFileSync(join(root, "src-tauri/tauri.conf.json"), JSON.stringify(config));
  return spawnSync(process.execPath, [join(root, "scripts/set-version.mjs"), "26.1.0-beta.8"], {
    cwd: root, encoding: "utf8", timeout: 15000,
  });
}

test("version command preserves the installer identity regardless of WiX field order", () => {
  for (const wix of [
    { upgradeCode: "72d4dea6-df92-56f0-bea9-784697c2e0e3", language: "ja-JP", version: "26.0.65535.30007" },
    { version: "26.0.65535.30007", upgradeCode: "72d4dea6-df92-56f0-bea9-784697c2e0e3", language: "ja-JP" },
  ]) {
    workspace(root => {
      const config = { identifier: "io.github.hikyousmp.hikyou-launcher", version: "../package.json", bundle: { windows: { wix } } };
      const result = runVersion(root, config);
      expect(result.error).toBeUndefined();
      expect(result.status).toBe(0);
      expect(JSON.parse(readFileSync(join(root, "package.json"), "utf8")).version).toBe("26.1.0-beta.8");
      const cargo = readFileSync(join(root, "src-tauri/Cargo.toml"), "utf8");
      expect(cargo).toContain('version = "26.1.0-beta.8"');
      expect(cargo).toContain('example = { version = "1" }');
      const updated = JSON.parse(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"));
      expect(updated).toEqual({ ...config, bundle: { windows: { wix: { ...wix, version: "26.0.65535.30008" } } } });
    });
  }
});

test("invalid WiX configuration fails before changing any version source", () => {
  workspace(root => {
    const config = { identifier: "fixture", bundle: { windows: { wix: { upgradeCode: "unchanged" } } } };
    const beforePackage = readFileSync(join(root, "package.json"), "utf8");
    const beforeCargo = readFileSync(join(root, "src-tauri/Cargo.toml"), "utf8");
    const result = runVersion(root, config);
    expect(result.error).toBeUndefined();
    expect(result.status).not.toBe(0);
    expect(result.stderr).toContain("Could not update the WiX version");
    expect(readFileSync(join(root, "package.json"), "utf8")).toBe(beforePackage);
    expect(readFileSync(join(root, "src-tauri/Cargo.toml"), "utf8")).toBe(beforeCargo);
    expect(JSON.parse(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"))).toEqual(config);
  });
});
