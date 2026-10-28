<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import ErrorBoundary from "./ErrorBoundary.svelte";
  import LogInspectorLoading from "./LogInspectorLoading.svelte";
  import { createTranslation } from "../lib/i18n.svelte";
  const { i18n } = createTranslation();
  const gameLog = import("./GameLog.svelte");
  onMount(() => {
    const unlisten = listen<string>("log://language-changed", ({ payload }) => {
      if (payload) i18n.changeLanguage(payload).catch(console.error);
    });
    return () => { unlisten.then((dispose) => dispose()).catch(console.error); };
  });
</script>

<ErrorBoundary>
  {#await gameLog}
    <LogInspectorLoading />
  {:then module}
    <module.default mode="log" />
  {:catch error}
    <div class="tool-window log-inspector-window p-6 text-danger">{error instanceof Error ? error.message : String(error)}</div>
  {/await}
</ErrorBoundary>
