import { createHash } from "node:crypto";
import { createServer } from "node:net";
import { spawn, spawnSync } from "node:child_process";
import { realpathSync } from "node:fs";
import { projectRoot } from "./build-privacy.mjs";

export function sessionEndpoint(root = projectRoot, platform = process.platform) {
  const key = realpathSync(root);
  const digest = createHash("sha256").update(platform === "win32" ? key.toLowerCase() : key).digest();
  // A Windows pipe does not compete with dynamic TCP ports used by OS services.
  return platform === "win32"
    ? { path: `\\\\.\\pipe\\hikyou-session-${digest.toString("hex")}`, exclusive: true }
    : { host: "127.0.0.1", port: 49152 + digest.readUInt16BE(0) % 16384, exclusive: true };
}

// Both endpoint types are OS-owned and release if the owner exits or is killed.
export async function acquireSession(root = projectRoot) {
  const endpoint = sessionEndpoint(root);
  const server = createServer((socket) => socket.destroy());
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(endpoint, resolve);
  }).catch((error) => {
    if (error.code === "EADDRINUSE") {
      throw new Error("Another dev/build session is active for this workspace, or its local session endpoint is occupied. Close that session before retrying.");
    }
    throw error;
  });
  return () => new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
}

export function runChild(command, args, options, signal) {
  if (signal.aborted) return Promise.resolve(130);
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { ...options, windowsHide: true, detached: process.platform !== "win32" });
    const stop = () => {
      if (!child.pid || child.exitCode !== null || child.signalCode !== null) return;
      if (process.platform === "win32") {
        spawnSync("taskkill", ["/PID", String(child.pid), "/T", "/F"], { stdio: "ignore", windowsHide: true });
      } else {
        try { process.kill(-child.pid, "SIGTERM"); } catch (error) { if (error.code !== "ESRCH") throw error; }
      }
    };
    signal.addEventListener("abort", stop, { once: true });
    child.once("error", (error) => { signal.removeEventListener("abort", stop); reject(error); });
    child.once("exit", (code) => {
      signal.removeEventListener("abort", stop);
      resolve(signal.aborted ? 130 : code ?? 1);
    });
    if (signal.aborted) stop();
  });
}

export async function runDev(args, options, signal, run = runChild, loadVite = () => import("vite")) {
  const { createServer } = await loadVite();
  const server = await createServer({
    root: projectRoot,
    server: { host: "127.0.0.1", port: 0, strictPort: true, open: false, ws: { host: "127.0.0.1" } },
  });
  try {
    await server.listen();
    if (signal.aborted) return 130;
    const address = server.httpServer?.address();
    if (!address || typeof address === "string") throw new Error("Vite did not provide a listening address.");
    const url = `http://127.0.0.1:${address.port}`;
    console.log(`Development server: ${url}`);
    const separator = args.indexOf("--");
    const split = separator < 0 ? args.length : separator;
    const override = JSON.stringify({ build: { devUrl: url, beforeDevCommand: "" } });
    const cliArgs = [...args.slice(0, split), "--config", override, ...args.slice(split)];
    return await run(process.execPath, cliArgs, options, signal);
  } finally {
    await server.close();
  }
}
