import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { projectRoot } from "./build-privacy.mjs";

const files = execFileSync("git", ["ls-files", "-z", "--cached", "--others", "--exclude-standard"], {
  cwd: projectRoot, encoding: "utf8",
}).split("\0").filter(Boolean);
const problems = [];
for (const file of files) {
  if (!existsSync(resolve(projectRoot, file))) continue;
  if (/(?:^|\/)\.env(?:\.|$)/.test(file) && !file.endsWith(".env.example") ||
      /\.(?:pfx|p12|pem|key|p8|keystore)$/i.test(file)) {
    problems.push(`${file}: private configuration or key material must not be tracked`);
    continue;
  }
  if (!/\.(?:rs|cs|tsx?|svelte|m?js|json|toml|ya?ml|md|html|css|plist|svg|csproj)$/i.test(file)) continue;
  const contents = readFileSync(resolve(projectRoot, file), "utf8");
  const workspaceSpellings = [projectRoot, projectRoot.replaceAll("\\", "/"),
    projectRoot.replaceAll("\\", "\\\\")];
  if (/[a-z]:[\\/]+Users[\\/]+[^\\/\s"']+/i.test(contents) ||
      /\/(?:Users|home)\/[A-Za-z0-9_.-]+/.test(contents) ||
      workspaceSpellings.some((path) => contents.toLowerCase().includes(path.toLowerCase()))) {
    problems.push(`${file}: a developer home or workspace path is embedded`);
  }
}
if (problems.length) throw new Error(`Privacy check failed:\n${problems.join("\n")}`);
console.log("Source privacy check passed (no tracked local env/keys or developer home paths).");
