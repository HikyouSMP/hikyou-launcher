import { mount, unmount } from "svelte";
import GameErrorBanner from "../../src/components/GameErrorBanner.svelte";

// Loaded only by the isolated browser scenario, never by the application entry.
export function mountLaunchError() {
  const target = document.createElement("div");
  target.style.cssText = "position:fixed;inset:80px 20px auto;z-index:90";
  document.body.append(target);
  const banner = mount(GameErrorBanner, {
    target,
    props: {
      error: "mod_resolution_limit: search states/projects; no partial solution accepted",
      onClose: () => { void unmount(banner).then(() => target.remove()); },
    },
  });
}
