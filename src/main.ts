import { mount, type Component } from "svelte";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import i18n from "./i18n";
import "./index.css";
import WindowRoot from "./WindowRoot.svelte";
import DesktopRequired from "./components/DesktopRequired.svelte";

const applications: Record<string, () => Promise<{ default: Component }>> = {
  main: () => import("./App.svelte"),
  "game-log": () => import("./components/GameLogWindowApp.svelte"),
};
const target = document.getElementById("root");
if (target) {
  if (!isTauri()) {
    mount(DesktopRequired, { target });
  } else {
    const load = applications[getCurrentWindow().label];
    if (load) {
      load().then(({ default: component }) => {
        mount(WindowRoot, { target, props: { component } });
      }).catch((error: unknown) => {
        console.error("Failed to load the application window", error);
        target.textContent = i18n.t("common.load_failed");
      });
    }
  }
}
