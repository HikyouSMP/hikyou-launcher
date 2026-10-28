import { expect, test } from "bun:test";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import { resolveUpdate } from "../src/lib/state.ts";
import { css } from "../src/lib/dom.ts";
import { javaRuntimeState, managedJavaName } from "../src/javaRuntimes.ts";
import { resolveSearchSuggestion } from "../src/search/suggestions.ts";
import { parseIntent } from "../src/utils/intent.ts";
import { CHIP_REVEAL_MOTION } from "../src/utils/chipMotion.ts";
import { useMatchedProfiles } from "../src/hooks/useMatchedProfiles.ts";
import { withSmartProfileDisplay } from "../src/utils/latestReleasePlus.ts";

const root = resolve(import.meta.dirname, "..");

test("smart search and display preserve the backend's completed target while newer versions exist", () => {
  const latest = { id: "smart:latest-plus", smartKey: "latest-plus", mcVersion: "1.21", loader: "auto", resolved: { mcVersion: "1.21", loader: "fabric", loaderVersion: "0.16" } };
  const snapshot = { id: "smart:snapshot-plus", smartKey: "snapshot-plus", mcVersion: "24w32a", loader: "auto", resolved: { mcVersion: "24w32a", loader: "fabric", loaderVersion: "0.16" } };
  expect(withSmartProfileDisplay(latest, { latest: { release: "1.22" } }).mcVersion).toBe("1.21");
  expect(useMatchedProfiles([latest, snapshot], parseIntent("latest"), "1.22").map(profile => profile.id)).toEqual([latest.id]);
  expect(useMatchedProfiles([latest, snapshot], parseIntent("snapshot"), "1.22").map(profile => profile.id)).toEqual([snapshot.id]);
});

test("chip spring is monotone, bounded, subpixel-accurate and lowers peak speed", () => {
  const stops = CHIP_REVEAL_MOTION.easing.slice(7, -1).split(",").map(Number);
  expect(stops.length).toBe(65);
  expect(stops[0]).toBe(0);
  expect(stops.at(-1)).toBe(1);
  for (let index = 1; index < stops.length; index++) {
    expect(stops[index]).toBeGreaterThanOrEqual(stops[index - 1]);
    expect(stops[index]).toBeLessThanOrEqual(1);
  }
  const duration = CHIP_REVEAL_MOTION.durationMs / 1000;
  const omega = Math.sqrt(1400);
  const response = seconds => 1 - (1 + omega * seconds) * Math.exp(-omega * seconds);
  let approximationError = 0;
  for (let index = 0; index <= 1024; index++) {
    const position = index / 1024 * 64;
    const stop = Math.min(63, Math.floor(position));
    const interpolated = stops[stop] + (stops[stop + 1] - stops[stop]) * (position - stop);
    approximationError = Math.max(approximationError,
      Math.abs(interpolated - response(index / 1024 * duration) / response(duration)));
  }
  expect(approximationError).toBeLessThan(0.003);
  const newPeak = Math.max(...stops.slice(1).map((value, index) => (value - stops[index]) * 64 / duration));
  // Baseline: the previous M3 standard Bezier, evaluated in its time domain.
  const previousCurve = time => {
    let lower = 0, upper = 1;
    for (let index = 0; index < 40; index++) {
      const u = (lower + upper) / 2;
      const x = 0.6 * u * (1 - u) ** 2 + u ** 3;
      if (x < time) lower = u; else upper = u;
    }
    const u = (lower + upper) / 2;
    return 3 * u ** 2 - 2 * u ** 3;
  };
  const oldPeak = Math.max(...Array.from({ length: 2000 }, (_, index) =>
    (previousCurve((index + 1) / 2000) - previousCurve(index / 2000)) * 2000 / 0.210));
  expect(newPeak).toBeLessThan(oldPeak * 0.8);
  console.info(`Chip motion comparison (normalized units/s, not comfort scores): old=${oldPeak.toFixed(3)}, new=${newPeak.toFixed(3)}, max_position_error=${approximationError.toFixed(6)}`);
});

test("structured search suggestions share canonical keywords and exact aliases", () => {
  for (const query of ["snapshot", "SNAPSHOT", "fabric snap", "ss"]) {
    expect(resolveSearchSuggestion(query, "release")).toEqual({
      id: "version-channel", action: "toggleVersionChannel", shortcut: "Tab", labelKey: "searchSuggestions.toSnapshots",
    });
  }
  expect(resolveSearchSuggestion("snapshot", "snapshot")?.labelKey).toBe("searchSuggestions.toReleases");
  for (const query of ["modpack", "mp", " MP "]) {
    expect(resolveSearchSuggestion(query, "release")).toEqual({
      id: "modpack-search", action: "searchModpacks", shortcut: "Space", labelKey: "searchSuggestions.searchModpacks",
    });
  }
  for (const query of ["", "1.21", "sn", "snapshotter", "https://snapshot.example", "modpacks", "mpack", "my mp world"]) {
    expect(resolveSearchSuggestion(query, "release")).toBeNull();
  }
  const definitions = JSON.parse(readFileSync(resolve(root, "src/search/suggestions.json"), "utf8"));
  expect(new Set(definitions.map(item => item.id)).size).toBe(definitions.length);
  for (const language of ["ja", "en"]) {
    const messages = JSON.parse(readFileSync(resolve(root, `src/locales/${language}.json`), "utf8"));
    for (const definition of definitions) {
      expect(definition.keyword).toBeTypeOf("string");
      expect(Array.isArray(definition.aliases)).toBe(true);
      if (definition.action === "toggleVersionChannel") {
        for (const word of [definition.keyword, ...definition.aliases]) expect(parseIntent(word).isSnap).toBe(true);
      }
      expect(["query", "token"]).toContain(definition.match);
      const shortcuts = { toggleVersionChannel: "Tab", searchModpacks: "Space" };
      expect(definition.shortcut).toBe(shortcuts[definition.action]);
      for (const key of Object.values(definition.labels ?? { default: definition.label })) {
        expect(key.split('.').reduce((value, part) => value?.[part], messages)).toBeTypeOf("string");
      }
    }
  }
});

test("Java status distinguishes defaults, excluded installations, cleanup and errors", () => {
  expect(javaRuntimeState({ selected: true, legacy: false, error: null })).toBe("default");
  expect(javaRuntimeState({ selected: false, legacy: true, error: null })).toBe("excluded");
  expect(javaRuntimeState({ selected: false, legacy: false, error: null })).toBe("cleanup");
  for (const legacy of [true, false]) {
    expect(javaRuntimeState({ selected: false, legacy, error: "missing binary" })).toBe("invalid");
  }
});

test("managed Zulu uses its product name without relabeling other vendors", () => {
  expect(managedJavaName({ directory: "zulu-21", vendor: "Azul Systems, Inc." })).toBe("Azul Zulu");
  expect(managedJavaName({ directory: "zulu-21", vendor: "Unknown" })).toBe("Unknown");
  expect(managedJavaName({ directory: "liberica-nik-21", vendor: "BellSoft" })).toBe("BellSoft");
});

test("frontend has no React runtime or JSX source", () => {
  const manifest = JSON.parse(readFileSync(resolve(root, "package.json"), "utf8"));
  const dependencies = { ...manifest.dependencies, ...manifest.devDependencies };
  for (const name of ["react", "react-dom", "react-i18next", "lucide-react", "framer-motion", "@vitejs/plugin-react", "@types/react", "@types/react-dom"]) {
    expect(dependencies[name]).toBeUndefined();
  }
  for (const entry of readdirSync(resolve(root, "src"), { recursive: true })) {
    expect(String(entry).endsWith(".tsx")).toBe(false);
    if (!/\.(?:ts|svelte)$/.test(String(entry))) continue;
    const source = readFileSync(resolve(root, "src", String(entry)), "utf8");
    expect(source).not.toMatch(/(?:from\s+|import\s*\(|require\s*\()["'](?:react(?:-dom|-i18next)?|lucide-react|framer-motion)(?:\/[^"']*)?["']/);
  }
});

test("state updates preserve value and functional update contracts", () => {
  expect(resolveUpdate(2, 5)).toBe(5);
  expect(resolveUpdate(2, (previous) => previous + 3)).toBe(5);
  const original = { enabled: false };
  expect(resolveUpdate(original, (previous) => ({ ...previous, enabled: true }))).toEqual({ enabled: true });
  expect(original.enabled).toBe(false);
});

test("retired skin and screenshot managers have no frontend entry or dependency", () => {
  const manifest = JSON.parse(readFileSync(resolve(root, "package.json"), "utf8"));
  expect(manifest.dependencies.skinview3d).toBeUndefined();
  expect(manifest.dependencies.three).toBeUndefined();
  for (const file of [
    "src/components/AppearanceView.svelte",
    "src/components/AppearancePlayerPreview.svelte",
    "src/components/ScreenshotGalleryApp.svelte",
    "src/components/ScreenshotImage.svelte",
    "src/hooks/useScreenshotGallery.svelte.ts",
  ]) expect(existsSync(resolve(root, file))).toBe(false);
  const entry = readFileSync(resolve(root, "src/main.ts"), "utf8");
  expect(entry).not.toMatch(/screenshots|Appearance/);
});

test("inline styles keep CSS units and custom properties", () => {
  expect(css({ width: 12, opacity: 0.5, marginTop: 0, "--offset": 3, display: undefined }))
    .toBe("width:12px;opacity:0.5;margin-top:0;--offset:3");
});
