import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const mode = process.argv[2];
const args = process.argv.slice(3);

try {
  if (mode === "prepare") {
    prepare(args[0]);
  } else if (mode === "publish") {
    publish();
  } else if (mode === "release") {
    const version = args.find((arg) => arg !== "--publish");
    if (!args.includes("--publish")) {
      fail("Publishing is a remote operation. Re-run with --publish after reviewing the target version.");
    }
    prepare(version);
    publish();
  } else {
    fail(
      "Usage:\n" +
        "  bun run release:prepare -- <semver>\n" +
        "  bun run release:publish\n" +
        "  bun run release -- <semver> --publish",
    );
  }
} catch (error) {
  console.error(`Release stopped: ${error instanceof Error ? error.message : String(error)}`);
  process.exitCode = 1;
}

function prepare(version) {
  validateVersion(version);
  assertMainBranch();
  assertCleanWorktree();

  const currentVersion = packageVersion();
  if (version === currentVersion) {
    fail(`Version ${version} is already current. Choose the next release version.`);
  }

  const ownedPaths = [
    "package.json",
    "src-tauri/Cargo.toml",
    "src-tauri/Cargo.lock",
    "src-tauri/tauri.conf.json",
  ];

  try {
    run("bun", ["scripts/set-version.mjs", version]);
    run("bun", ["run", "check:version"]);
    run("bun", ["run", "test:version"]);
    run("bun", ["run", "build"]);
    run("cargo", ["check", "--manifest-path", "src-tauri/Cargo.toml"]);
    run("cargo", ["fmt", "--manifest-path", "src-tauri/Cargo.toml", "--check"]);
    run("cargo", [
      "clippy",
      "--manifest-path",
      "src-tauri/Cargo.toml",
      "--all-targets",
      "--",
      "-D",
      "warnings",
    ]);
    run("cargo", ["test", "--manifest-path", "src-tauri/Cargo.toml", "--lib"]);
    run("git", ["diff", "--check"]);
    assertOnlyExpectedChanges(ownedPaths);
    run("git", ["add", "--", ...ownedPaths]);
    run("git", ["commit", "-m", `chore(release): prepare v${version}`]);
  } catch (error) {
    restoreOwnedPaths(ownedPaths);
    throw error;
  }

  assertCleanWorktree();
  console.log(`Release v${version} is prepared locally. Review the commit, then run bun run release:publish.`);
}

function publish() {
  assertMainBranch();
  assertCleanWorktree();
  run("bun", ["run", "check:version"]);

  const version = packageVersion();
  const tag = `v${version}`;
  const head = capture("git", ["rev-parse", "HEAD"]);
  const committedVersion = JSON.parse(capture("git", ["show", "HEAD:package.json"])).version;
  if (committedVersion !== version) {
    fail(`HEAD contains ${committedVersion}, but the working tree contains ${version}.`);
  }

  const localTagCommit = resolvedLocalTag(tag);
  if (localTagCommit && localTagCommit !== head) {
    fail(`${tag} already points to ${localTagCommit.slice(0, 7)}, not HEAD ${head.slice(0, 7)}.`);
  }
  if (remoteTagExists(tag)) {
    fail(`${tag} already exists on origin. Refusing to replace a published tag.`);
  }

  run("git", ["push", "origin", "main"]);
  if (!localTagCommit) {
    run("git", ["tag", "-a", tag, "-m", `Hikyou Launcher ${tag}`]);
  }
  run("git", ["push", "origin", tag]);
  console.log(`Published ${tag}. GitHub Actions will build the tagged commit ${head.slice(0, 7)}.`);
}

function validateVersion(version) {
  if (!version || !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(version)) {
    fail("A valid SemVer version is required, for example 26.1.0-beta.3.");
  }
}

function packageVersion() {
  return JSON.parse(readFileSync(resolve(root, "package.json"), "utf8")).version;
}

function assertMainBranch() {
  const branch = capture("git", ["branch", "--show-current"]);
  if (branch !== "main") {
    fail(`Releases must be prepared from main; current branch is ${branch || "detached HEAD"}.`);
  }
}

function assertCleanWorktree() {
  const status = capture("git", ["status", "--porcelain"]);
  if (status) {
    fail("The worktree must be clean before a release operation.");
  }
}

function assertOnlyExpectedChanges(expectedPaths) {
  const changed = capture("git", ["status", "--porcelain"])
    .split("\n")
    .filter(Boolean)
    .map((line) => line.slice(3).replaceAll("\\", "/"));
  const unexpected = changed.filter((path) => !expectedPaths.includes(path));
  if (unexpected.length > 0) {
    fail(`Release checks changed unexpected files:\n${unexpected.join("\n")}`);
  }
}

function restoreOwnedPaths(paths) {
  const result = spawnSync("git", ["restore", "--staged", "--worktree", "--", ...paths], {
    cwd: root,
    stdio: "inherit",
    shell: false,
  });
  if (result.status !== 0) {
    console.error("Release preparation failed and automatic restoration was incomplete.");
  }
}

function resolvedLocalTag(tag) {
  const result = spawnSync("git", ["rev-list", "-n", "1", tag], {
    cwd: root,
    encoding: "utf8",
    shell: false,
  });
  return result.status === 0 ? result.stdout.trim() : "";
}

function remoteTagExists(tag) {
  const result = spawnSync("git", ["ls-remote", "--exit-code", "--tags", "origin", `refs/tags/${tag}`], {
    cwd: root,
    encoding: "utf8",
    shell: false,
  });
  if (result.status === 0) return true;
  if (result.status === 2) return false;
  fail(`Could not verify whether ${tag} exists on origin.\n${result.stderr.trim()}`);
}

function run(command, commandArgs) {
  const result = spawnSync(command, commandArgs, {
    cwd: root,
    stdio: "inherit",
    shell: false,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`${command} ${commandArgs.join(" ")} failed with exit code ${result.status}.`);
  }
}

function capture(command, commandArgs) {
  const result = spawnSync(command, commandArgs, {
    cwd: root,
    encoding: "utf8",
    shell: false,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(result.stderr.trim() || `${command} ${commandArgs.join(" ")} failed.`);
  }
  return result.stdout.trim();
}

function fail(message) {
  throw new Error(message);
}
