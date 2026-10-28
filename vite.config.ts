import { defineConfig } from "vite";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";

const host = process.env.TAURI_DEV_HOST;
const require = createRequire(import.meta.url);

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [svelte(), tailwindcss(), {
    name: "bundled-font-licenses",
    generateBundle() {
      for (const font of ["inter", "jetbrains-mono", "noto-sans-jp"]) {
        this.emitFile({
          type: "asset",
          fileName: `licenses/${font}.txt`,
          source: readFileSync(require.resolve(`@fontsource-variable/${font}/LICENSE`)),
        });
      }
    },
  }],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  optimizeDeps: { entries: ["index.html"] },
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    ws: host
      ? {
          protocol: "ws",
          host,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    rolldownOptions: {
      checks: {
        pluginTimings: false,
      },
      output: {
        codeSplitting: true,
        manualChunks(id: string) {
          if (!id.includes("node_modules")) return undefined;
          if (id.includes("node_modules/svelte/") || id.includes("node_modules\\svelte\\")) return "svelte";
          if (id.includes("@lucide/svelte")) return "icons";
          if (id.includes("i18next")) {
            return "i18n";
          }
          return "vendor";
        },
      },
    },
  },
}));
