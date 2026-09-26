import {
  copyFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { execFileSync } from "node:child_process";
import { basename, join, resolve, sep } from "node:path";
import { verifyAuthBroker } from "./verify-auth-broker.mjs";

if (process.platform !== "win32") process.exit(0);

const root = join(import.meta.dirname, "..");
const triple = (process.env.CARGO_BUILD_TARGET ||
  execFileSync("rustc", ["--print", "host-tuple"], { encoding: "utf8" })).trim();
const rid = triple.startsWith("aarch64-") ? "win-arm64" : "win-x64";
const target = resolve(root, "src-tauri", "target");
const output = resolve(target, "auth-broker", rid);
const binaries = join(root, "src-tauri", "binaries");
const destination = join(binaries, `hikyou-auth-broker-${triple}.exe`);
const bundledRuntime = join(binaries, "auth-broker-runtime");
const generatedConfig = join(
  root,
  "src-tauri",
  "auth-broker",
  "GeneratedAuthConfig.cs",
);
const clientId = (process.env.HIKYOU_MSA_CLIENT_ID || "").trim();
const placeholderId = "00000000-0000-0000-0000-000000000000";

if (
  clientId &&
  (clientId === placeholderId ||
    !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(clientId))
) {
  throw new Error("HIKYOU_MSA_CLIENT_ID must be a Microsoft application UUID");
}

writeFileSync(
  generatedConfig,
  `internal static class GeneratedAuthConfig\n{\n    internal const string ClientId = ${JSON.stringify(clientId)};\n}\n`,
  "utf8",
);

for (const directory of [output, bundledRuntime]) {
  const resolved = resolve(directory);
  const allowedRoot = directory === output ? target : resolve(binaries);
  if (!resolved.startsWith(`${allowedRoot}${sep}`)) {
    throw new Error(`Refusing to clear unexpected auth broker path: ${resolved}`);
  }
  rmSync(resolved, { recursive: true, force: true });
}

execFileSync("dotnet", [
  "publish",
  join(root, "src-tauri", "auth-broker", "Hikyou.AuthBroker.csproj"),
  "--configuration", "Release",
  "--runtime", rid,
  "--self-contained", "true",
  "--output", output,
], { stdio: "inherit" });

const source = join(output, "hikyou-auth-broker.exe");
if (!existsSync(source)) throw new Error(`Auth broker was not produced: ${source}`);
mkdirSync(binaries, { recursive: true });
copyFileSync(source, destination);
const runtimeName = rid === "win-arm64" ? "msalruntime_arm64.dll" : "msalruntime.dll";
const runtimeSource = join(output, runtimeName);
if (!existsSync(runtimeSource)) throw new Error(`Auth runtime was not produced: ${runtimeSource}`);
mkdirSync(bundledRuntime, { recursive: true });
for (const entry of readdirSync(output, { withFileTypes: true })) {
  if (entry.name === basename(source) || entry.name === runtimeName || entry.name.endsWith(".pdb")) {
    continue;
  }
  cpSync(join(output, entry.name), join(bundledRuntime, entry.name), {
    recursive: entry.isDirectory(),
  });
}
const authRuntime = join(bundledRuntime, "auth-runtime");
mkdirSync(authRuntime, { recursive: true });
copyFileSync(runtimeSource, join(authRuntime, runtimeName));
if ((rid === "win-arm64" ? "arm64" : "x64") !== process.arch) {
  throw new Error("Auth broker verification requires a runner matching the target architecture");
}
verifyAuthBroker(destination, bundledRuntime, runtimeName);
console.log(`Prepared ${destination}`);
