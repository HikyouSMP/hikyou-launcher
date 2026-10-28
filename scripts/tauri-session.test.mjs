import { expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createServer } from "node:net";
import { acquireSession, runChild, runDev } from "./tauri-session.mjs";

test("workspace session rejects duplicate and releases for the next startup", async () => {
  const directory = mkdtempSync(join(tmpdir(), "hikyou-session-"));
  let release;
  try {
    release = await acquireSession(directory);
    await expect(acquireSession(directory)).rejects.toThrow("Another dev/build session");
    await release();
    release = await acquireSession(directory);
  } finally {
    await release?.();
    rmSync(directory, { recursive: true });
  }
});

function fixture(failListen = false) {
  const events = [];
  const server = {
    listen: async () => { events.push("listen"); if (failListen) throw new Error("listen failed"); },
    close: async () => { events.push("close"); },
    httpServer: { address: () => ({ port: 54321 }) },
  };
  const load = async () => ({ createServer: async (config) => {
    expect(config.server.host).toBe("127.0.0.1");
    expect(config.server.port).toBe(0);
    return server;
  } });
  return { events, load };
}

test("dev passes the bound Vite URL to Tauri and preserves runner arguments", async () => {
  const { events, load } = fixture();
  const status = await runDev(["tauri.js", "dev", "--config", "custom.json", "--", "--locked"], {}, new AbortController().signal,
    async (_, args) => {
      expect(events).toEqual(["listen"]);
      const config = JSON.parse(args[args.indexOf("--") - 1]);
      expect(config.build).toEqual({ devUrl: "http://127.0.0.1:54321", beforeDevCommand: "" });
      expect(args.slice(-2)).toEqual(["--", "--locked"]);
      events.push("tauri");
      return 7;
    }, load);
  expect(status).toBe(7);
  expect(events).toEqual(["listen", "tauri", "close"]);
});

test("Vite closes on startup error, child spawn error and cancellation", async () => {
  for (const failListen of [true, false]) {
    const { events, load } = fixture(failListen);
    await expect(runDev(["tauri.js", "dev"], {}, new AbortController().signal,
      async () => { throw new Error("spawn failed"); }, load)).rejects.toThrow(failListen ? "listen failed" : "spawn failed");
    expect(events.at(-1)).toBe("close");
  }
  const { events, load } = fixture();
  const controller = new AbortController();
  controller.abort();
  expect(await runDev(["tauri.js", "dev"], {}, controller.signal,
    () => { throw new Error("must not spawn"); }, load)).toBe(130);
  expect(events).toEqual(["listen", "close"]);
});

test("child runner preserves failures and terminates its owned process on cancellation", async () => {
  const controller = new AbortController();
  expect(await runChild(process.execPath, ["-e", "process.exit(7)"], { stdio: "ignore" }, controller.signal)).toBe(7);
  const running = runChild(process.execPath, ["-e", "setInterval(() => {}, 1000)"], { stdio: "ignore" }, controller.signal);
  controller.abort();
  expect(await running).toBe(130);
});

test("real managed Vite works with 1420 occupied and releases its actual port", async () => {
  const occupied = createServer((socket) => socket.destroy());
  try {
    await new Promise((resolve, reject) => {
      occupied.once("error", (error) => error.code === "EADDRINUSE" ? resolve() : reject(error));
      occupied.listen({ host: "127.0.0.1", port: 1420, exclusive: true }, resolve);
    });
    for (let attempt = 0; attempt < 2; attempt++) {
      let url;
      const status = await runDev(["tauri.js", "dev"], {}, new AbortController().signal, async (_, args) => {
        url = JSON.parse(args.at(-1)).build.devUrl;
        expect(new URL(url).port).not.toBe("1420");
        for (const path of ["/", "/@vite/client", "/src/main.ts"]) {
          // A cold dependency prebundle can outlast ordinary HTTP requests.
          const response = await fetch(url + path, { signal: AbortSignal.timeout(90000) })
            .catch(error => { throw new Error(`Dev server failed to serve ${path}`, { cause: error }); });
          expect(response.status).toBe(200);
          await response.text();
        }
        return 0;
      });
      expect(status).toBe(0);
      // Close must release the exact listener, not merely terminate a launcher shell.
      const probe = createServer();
      await new Promise((resolve, reject) => {
        probe.once("error", reject);
        probe.listen({ host: "127.0.0.1", port: Number(new URL(url).port), exclusive: true }, resolve);
      });
      await new Promise((resolve) => probe.close(resolve));
    }
  } finally {
    if (occupied.listening) await new Promise((resolve) => occupied.close(resolve));
  }
}, 240000);
