import { expect, test } from "bun:test";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { checkWindowsBundleInputs } from "./check-bundle-inputs.mjs";

test("Windows packaging rejects any stale DLL, not just known authentication DLL names", () => {
  const directory = mkdtempSync(join(tmpdir(), "hikyou-bundle-test-"));
  try {
    writeFileSync(join(directory, "application.exe"), "test fixture");
    expect(() => checkWindowsBundleInputs(directory)).not.toThrow();
    for (const name of ["coreclr.dll", "other-native-library.DLL"]) {
      const file = join(directory, name);
      writeFileSync(file, "test fixture");
      expect(() => checkWindowsBundleInputs(directory)).toThrow("Unexpected DLLs");
      rmSync(file);
    }
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
