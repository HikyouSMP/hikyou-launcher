import { copyFileSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { join } from "node:path";

if (process.platform !== "win32") process.exit(0);

const root = join(import.meta.dirname, "..");
const triple = (process.env.CARGO_BUILD_TARGET ||
  execFileSync("rustc", ["--print", "host-tuple"], { encoding: "utf8" })).trim();
const rid = triple.startsWith("aarch64-") ? "win-arm64" : "win-x64";
const output = join(root, "src-tauri", "target", "auth-broker", rid);
const binaries = join(root, "src-tauri", "binaries");
const destination = join(binaries, `hikyou-auth-broker-${triple}.exe`);
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
console.log(`Prepared ${destination}`);
