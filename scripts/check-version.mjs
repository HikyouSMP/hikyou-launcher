import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const packageJson = JSON.parse(readFileSync(resolve(root, "package.json"), "utf8"));
const cargoToml = readFileSync(resolve(root, "src-tauri", "Cargo.toml"), "utf8");
const tauriConfig = JSON.parse(
  readFileSync(resolve(root, "src-tauri", "tauri.conf.json"), "utf8"),
);

const cargoVersion = cargoToml.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1];
if (!cargoVersion) {
  throw new Error("Could not read the package version from src-tauri/Cargo.toml.");
}
if (tauriConfig.version !== "../package.json") {
  throw new Error("Tauri must read its app version from ../package.json.");
}
if (cargoVersion !== packageJson.version) {
  throw new Error(
    `Version mismatch: package.json=${packageJson.version}, Cargo.toml=${cargoVersion}`,
  );
}

console.log(`Version sources are aligned: ${packageJson.version}`);
