import assert from "node:assert/strict";
import test from "node:test";
import { installIsolation } from "./smoke.mjs";

async function harness(options) {
  let http;
  let websocket;
  const evidence = { blockedRequests: [], routeErrors: [], entryInjections: 0, closing: false };
  await installIsolation({
    async route(_pattern, handler) { http = handler; },
    async routeWebSocket(_pattern, handler) { websocket = handler; },
  }, "http://127.0.0.1:1420", evidence, options);
  return { http, websocket, evidence };
}

function request(url, overrides = {}) {
  const calls = [];
  return {
    calls,
    request: () => ({ url: () => url }),
    async abort(reason) { calls.push(["abort", reason]); },
    async continue() { calls.push(["continue"]); },
    ...overrides,
  };
}

test("entry injection fetches the original module and only prepends the fixture import", async () => {
  const { http, evidence } = await harness();
  let fulfilled;
  const route = request("http://127.0.0.1:1420/src/main.ts?t=1", {
    async fetch(options) {
      assert.deepEqual(options, { timeout: 15000, maxRedirects: 0 });
      return {
        status: () => 200,
        text: async () => "console.log('production entry');",
        headers: () => ({ "content-length": "32", "content-encoding": "gzip" }),
      };
    },
    async fulfill(response) { fulfilled = response; },
  });
  await http(route);
  assert.equal(fulfilled.body, "import '/tests/frontend/fixture.ts';\nconsole.log('production entry');");
  assert.equal(fulfilled.headers["content-length"], undefined);
  assert.equal(fulfilled.headers["content-encoding"], undefined);
  assert.equal(evidence.entryInjections, 1);
  assert.deepEqual(evidence.routeErrors, []);
});

test("entry connection resets become evidence and an aborted request, not a rejected callback", async () => {
  const { http, evidence } = await harness();
  const route = request("http://127.0.0.1:1420/src/main.ts", {
    async fetch() { throw new Error("ECONNRESET"); },
  });
  await assert.doesNotReject(() => http(route));
  assert.deepEqual(route.calls, [["abort", "failed"]]);
  assert.match(evidence.routeErrors[0].error, /ECONNRESET/);
  assert.equal(evidence.entryInjections, 0);
});

test("ordinary browser entry is not replaced with a Tauri fixture", async () => {
  const { http, evidence } = await harness({ injectFixture: false });
  const route = request("http://127.0.0.1:1420/src/main.ts");
  await http(route);
  assert.deepEqual(route.calls, [["continue"]]);
  assert.equal(evidence.entryInjections, 0);
});

test("closing contexts absorb both fetch and abort disposal failures", async () => {
  const { http, evidence } = await harness();
  evidence.closing = true;
  await assert.doesNotReject(() => http(request("http://127.0.0.1:1420/src/main.ts", {
    async fetch() { throw new Error("Request context disposed"); },
    async abort() { throw new Error("Target closed"); },
  })));
  assert.deepEqual(evidence.routeErrors, []);
});

test("HTTP external requests are blocked while same-origin modules continue", async () => {
  const { http, evidence } = await harness();
  const external = request("https://fixture.invalid/asset.png");
  const local = request("http://127.0.0.1:1420/src/App.svelte");
  await http(external);
  await http(local);
  assert.deepEqual(external.calls, [["abort", "blockedbyclient"]]);
  assert.deepEqual(local.calls, [["continue"]]);
  assert.deepEqual(evidence.blockedRequests, [{ url: "https://fixture.invalid/asset.png", kind: "http" }]);
});

test("unexpected same-origin routing failures are also recorded", async () => {
  const { http, evidence } = await harness();
  const route = request("http://127.0.0.1:1420/src/App.svelte", {
    async continue() { throw new Error("Target disappeared"); },
  });
  await assert.doesNotReject(() => http(route));
  assert.deepEqual(route.calls, [["abort", "failed"]]);
  assert.equal(evidence.routeErrors[0].error, "Target disappeared");
});

test("only same-origin WebSockets can connect", async () => {
  const { websocket, evidence } = await harness();
  let connected = false;
  let closed;
  websocket({ url: () => "ws://127.0.0.1:1420/", connectToServer() { connected = true; } });
  websocket({ url: () => "wss://fixture.invalid/", close(options) { closed = options; } });
  assert.equal(connected, true);
  assert.equal(closed.code, 1008);
  assert.deepEqual(evidence.blockedRequests, [{ url: "wss://fixture.invalid/", kind: "websocket" }]);
});
