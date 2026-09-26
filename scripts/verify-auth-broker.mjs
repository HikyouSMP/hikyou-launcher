import { copyFileSync, cpSync, mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { basename, dirname, join, resolve } from "node:path";
import { tmpdir } from "node:os";
import assert from "node:assert/strict";

export function verifyAuthBroker(executable, bundledRuntime, runtimeName) {
  const directory = mkdtempSync(join(tmpdir(), "hikyou-broker-check-"));
  try {
    const stage = (name) => {
      const root = join(directory, name);
      mkdirSync(root);
      copyFileSync(executable, join(root, "hikyou-auth-broker.exe"));
      cpSync(bundledRuntime, root, { recursive: true });
      return root;
    };
    const completeDirectory = stage("complete");
    const missingDirectory = stage("missing-native-runtime");
    const declaredRuntime = join(missingDirectory, "auth-runtime", runtimeName);
    // A DLL in the working directory must not hide a broken installation.
    copyFileSync(declaredRuntime, join(missingDirectory, basename(declaredRuntime)));
    rmSync(join(missingDirectory, "auth-runtime"), { recursive: true, force: true });
    const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => key.toLowerCase() !== "path"));
    env.PATH = join(process.env.SystemRoot, "System32");
    const check = (root) => {
      const result = spawnSync(join(root, "hikyou-auth-broker.exe"), ["--self-test"], {
        cwd: root, env, encoding: "utf8", timeout: 15000, windowsHide: true,
      });
      if (result.error) throw result.error;
      assert.equal(result.signal, null, "Broker self-test was terminated");
      return { status: result.status, response: JSON.parse(result.stdout) };
    };
    const missing = check(missingDirectory);
    assert.equal(missing.status, 3);
    assert.equal(missing.response.error, "runtime_missing");
    assert.deepEqual(check(completeDirectory), { status: 0, response: { status: "ok" } });
    console.log("Auth broker distribution check passed (self-contained files complete; misplaced native runtime rejected; clean PATH succeeds).");
  } finally {
    assert.equal(dirname(resolve(directory)), resolve(tmpdir()));
    rmSync(directory, { recursive: true, force: true });
  }
}
