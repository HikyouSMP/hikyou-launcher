import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const packageJson = JSON.parse(readFileSync(resolve(root, "package.json"), "utf8"));
const tag = process.env.GITHUB_REF_NAME;
const expectedTag = `v${packageJson.version}`;

if (!tag) {
  throw new Error("GITHUB_REF_NAME is required to validate a release tag.");
}
if (tag !== expectedTag) {
  throw new Error(`Release tag mismatch: expected ${expectedTag}, received ${tag}.`);
}

console.log(`Release tag matches the app version: ${tag}`);
