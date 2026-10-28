import { closeSync, existsSync, openSync, readSync, realpathSync } from "node:fs";
import { homedir } from "node:os";
import { resolve } from "node:path";

export const projectRoot = resolve(import.meta.dirname, "..");

function spellings(path) {
  const paths = new Set([resolve(path)]);
  if (existsSync(path)) paths.add(realpathSync.native(path));
  return [...paths].flatMap((value) => [value, value.replaceAll("\\", "/")]);
}

export function privateBuildRoots(env = process.env) {
  return [
    [homedir(), "/_/home"],
    [env.CARGO_HOME || resolve(homedir(), ".cargo"), "/_/cargo"],
    [env.RUSTUP_HOME || resolve(homedir(), ".rustup"), "/_/rustup"],
    [projectRoot, "/_/hikyou"],
  ];
}

export function privateBuildEnv(env = process.env) {
  const flags = env.CARGO_ENCODED_RUSTFLAGS !== undefined
    ? env.CARGO_ENCODED_RUSTFLAGS.split("\x1f").filter(Boolean)
    : (env.RUSTFLAGS || "").split(/\s+/).filter(Boolean);
  for (const [path, replacement] of privateBuildRoots(env)) {
    for (const prefix of spellings(path)) {
      flags.push(`--remap-path-prefix=${prefix}=${replacement}`);
    }
  }
  const result = { ...env, CARGO_ENCODED_RUSTFLAGS: flags.join("\x1f") };
  if (process.platform === "win32") {
    // MSVC emits absolute header paths in native assertions, outside rustc's remapping.
    // Keep assertions enabled; the artifact gate detects unsupported compiler behavior.
    const prefix = homedir().replaceAll("\\", "/") + "/";
    for (const key of ["CFLAGS", "CXXFLAGS"]) {
      result[key] = `${env[key] || ""} /d1trimfile:"${prefix}"`.trim();
    }
    result.CC_SHELL_ESCAPED_FLAGS = "1";
  } else {
    const nativeFlags = privateBuildRoots(env).flatMap(([root, replacement]) =>
      spellings(root).map((prefix) => `"-ffile-prefix-map=${prefix}=${replacement}"`),
    ).join(" ");
    for (const key of ["CFLAGS", "CXXFLAGS"]) {
      result[key] = `${env[key] || ""} ${nativeFlags}`.trim();
    }
    result.CC_SHELL_ESCAPED_FLAGS = "1";
  }
  return result;
}

// Check both native UTF-8 and Windows UTF-16 strings without loading a
// complete executable into memory. Report the file only, never the private path.
export function assertPrivateArtifact(path, roots = privateBuildRoots()) {
  const needles = roots.flatMap(([root]) => spellings(root).flatMap((value) => [
    foldAscii(Buffer.from(value, "utf8")),
    foldAscii(Buffer.from(value, "utf16le")),
  ]));
  const overlap = Math.max(...needles.map((needle) => needle.length)) - 1;
  const descriptor = openSync(path, "r");
  const chunk = Buffer.alloc(1024 * 1024);
  let tail = Buffer.alloc(0);
  try {
    for (;;) {
      const length = readSync(descriptor, chunk);
      if (!length) break;
      const bytes = Buffer.concat([tail, chunk.subarray(0, length)]);
      // ASCII case folding preserves byte offsets, including UTF-16 strings.
      const folded = foldAscii(bytes);
      if (needles.some((needle) => folded.includes(needle))) {
        throw new Error("Build artifact contains a private build path; distribution stopped.");
      }
      tail = bytes.subarray(Math.max(0, bytes.length - overlap));
    }
  } finally {
    closeSync(descriptor);
  }
}

function foldAscii(bytes) {
  const folded = Buffer.from(bytes);
  for (let index = 0; index < folded.length; index++) {
    if (folded[index] >= 65 && folded[index] <= 90) folded[index] += 32;
  }
  return folded;
}
