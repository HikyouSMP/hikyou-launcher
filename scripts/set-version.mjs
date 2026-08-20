import { readFileSync, renameSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const version = process.argv[2];
if (!version || !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(version)) {
  throw new Error("Usage: bun run set:version -- <semver>");
}

const root = resolve(import.meta.dirname, "..");
const packagePath = resolve(root, "package.json");
const cargoPath = resolve(root, "src-tauri", "Cargo.toml");
const packageJson = JSON.parse(readFileSync(packagePath, "utf8"));
const cargoToml = readFileSync(cargoPath, "utf8");
const cargoLines = cargoToml.split(/(?<=\n)/);
let inPackageSection = false;
let foundPackageVersion = false;
const updatedCargoToml = cargoLines
  .map((line) => {
    const section = line.trim();
    if (section.startsWith("[") && section.endsWith("]")) {
      inPackageSection = section === "[package]";
    }
    if (inPackageSection && /^version\s*=\s*"[^"]+"\s*$/.test(section)) {
      foundPackageVersion = true;
      return line.replace(/"[^"]+"/, `"${version}"`);
    }
    return line;
  })
  .join("");

if (!foundPackageVersion) {
  throw new Error("Could not update the package version in src-tauri/Cargo.toml.");
}

packageJson.version = version;
writeAtomically(packagePath, `${JSON.stringify(packageJson, null, 2)}\n`);
writeAtomically(cargoPath, updatedCargoToml);

console.log(`Version updated: ${version}`);

function writeAtomically(path, contents) {
  const temporaryPath = `${path}.tmp`;
  writeFileSync(temporaryPath, contents, "utf8");
  renameSync(temporaryPath, path);
}
