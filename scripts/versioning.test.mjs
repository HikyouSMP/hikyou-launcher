import { expect, test } from "bun:test";
import { deriveWixVersion } from "./versioning.mjs";

test("uses the stable application version for MSI", () => {
  expect(deriveWixVersion("26.1.0")).toBe("26.1.0");
});

test("orders beta releases below their eventual stable release", () => {
  expect(deriveWixVersion("26.1.0-beta.1")).toBe("26.0.65535.30001");
  expect(deriveWixVersion("26.1.0-beta.2")).toBe("26.0.65535.30002");
});

test("orders release candidates after beta releases", () => {
  expect(deriveWixVersion("26.1.0-rc.1")).toBe("26.0.65535.50001");
});

test("rejects pre-release formats that cannot preserve MSI ordering", () => {
  expect(() => deriveWixVersion("26.1.0-preview.1")).toThrow();
});
