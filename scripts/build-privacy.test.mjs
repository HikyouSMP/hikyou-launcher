import { expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { assertPrivateArtifact, privateBuildEnv, projectRoot } from "./build-privacy.mjs";

test("encoded build flags preserve spaces and caller flags", () => {
  const environment = privateBuildEnv({ CARGO_ENCODED_RUSTFLAGS: "-C\x1fopt-level=2" });
  const flags = environment.CARGO_ENCODED_RUSTFLAGS.split("\x1f");
  expect(flags.slice(0, 2)).toEqual(["-C", "opt-level=2"]);
  expect(flags).toContain(`--remap-path-prefix=${projectRoot}=/_/hikyou`);
  expect(environment.CC_SHELL_ESCAPED_FLAGS).toBe("1");
  expect(environment.CFLAGS).toContain(process.platform === "win32" ? "/d1trimfile:" : "-ffile-prefix-map=");
});

test("artifact privacy detects native and UTF-16 paths across read boundaries", () => {
  const directory = mkdtempSync(join(tmpdir(), "hikyou-build-privacy-"));
  const artifact = join(directory, "sample.bin");
  try {
    for (const encoding of ["utf8", "utf16le"]) {
      writeFileSync(artifact, Buffer.concat([
        Buffer.alloc(1024 * 1024 - 3), Buffer.from(directory, encoding),
      ]));
      expect(() => assertPrivateArtifact(artifact, [[directory, "/_/test"]])).toThrow();
    }
    writeFileSync(artifact, "/_/hikyou/src/main.rs");
    expect(() => assertPrivateArtifact(artifact, [[directory, "/_/test"]])).not.toThrow();
  } finally {
    rmSync(directory, { recursive: true });
  }
});
