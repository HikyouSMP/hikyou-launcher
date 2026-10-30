import { expect, test } from "bun:test";
import { readFileSync, readdirSync } from "node:fs";
import { resolve, join } from "node:path";

const root = resolve(import.meta.dirname, "..");
const read = path => readFileSync(join(root, path), "utf8");

test("canonical bundle identity preserves the existing installer family", () => {
  const config = JSON.parse(read("src-tauri/tauri.conf.json"));
  expect(config.identifier).toBe("io.github.hikyousmp.hikyou-launcher");
  expect(config.productName).toBe("Hikyou Launcher");
  expect(config.bundle.windows.wix.upgradeCode).toBe("72d4dea6-df92-56f0-bea9-784697c2e0e3");
  expect(read("README.md")).toContain(config.identifier);
  expect(read("docs/runtime-boundaries.md")).toContain(config.identifier);
  const macos = read("src-tauri/src/auth/crypto/macos.rs");
  expect(macos).toContain('LEGACY_KEYCHAIN_ACCESS_GROUP');
  expect(macos).toContain('"io.github.hikyou-smp.hikyou-launcher"');
});

test("public Markdown uses the current GitHub organization", () => {
  const files = ["README.md", "CONTRIBUTING.md", "SECURITY.md", "AGENTS.md"];
  const visit = directory => {
    for (const entry of readdirSync(join(root, directory), { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) visit(path);
      else if (entry.isFile() && entry.name.endsWith(".md")) files.push(path);
    }
  };
  visit("docs");
  for (const file of files) {
    expect(read(file).toLowerCase()).not.toContain("github.com/hikyou-smp/");
  }
});
