import { spawnSync } from "node:child_process";
import { statSync } from "node:fs";
import { isAbsolute, join } from "node:path";

const runtimes = [8, 17, 21, 25].map((major) => {
  const home = process.env[`HIKYOU_JAVA_${major}`];
  if (!home || !isAbsolute(home)) throw new Error(`Explicit JDK ${major} home is required`);
  const java = join(home, "bin", process.platform === "win32" ? "java.exe" : "java");
  if (!statSync(java).isFile()) throw new Error(`JDK ${major} executable is missing`);
  return { major, java };
});

function check(test, runtime) {
  const result = spawnSync("cargo", [
    "test", "--locked", "--manifest-path", "src-tauri/Cargo.toml", "--lib", test,
    "--", "--ignored", "--nocapture",
  ], {
    stdio: "inherit",
    timeout: 15 * 60 * 1000,
    env: { ...process.env, HIKYOU_TEST_JAVA: runtime.java, HIKYOU_TEST_JAVA_MAJOR: String(runtime.major) },
  });
  if (result.error || result.status !== 0) {
    throw new Error(`${test} failed for Java ${runtime.major}`, { cause: result.error });
  }
}

for (const runtime of runtimes) check("jvm_probe::tests::live_owned_flags", runtime);
const oracle = runtimes.find(({ major }) => major === 17);
if (!oracle) throw new Error("Fabric reference JDK is missing");
check("mods::fabric_oracle::official_fabric_predicates_agree", oracle);
