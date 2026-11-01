<script lang="ts">
  import { mockIPC } from "@tauri-apps/api/mocks";
  import RecModsPanel from "../../src/components/RecModsPanel.svelte";
  import type { AutoMod } from "../../src/types";

  const icon = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aC1sAAAAASUVORK5CYII=";
  const calls: string[] = [];
  let metadataRequests = $state(0);
  let saves = $state(0);
  mockIPC((command, args) => {
    calls.push(command);
    if (command === "fetch_cached_icon") {
      if (args && typeof args === "object" && "url" in args && "projectId" in args && args.url === null && args.projectId === "mod-0") metadataRequests++;
      return { data_url: icon, warning: null };
    }
    if (command === "search_modrinth") return [{ project_id: "search-result", title: "Search result", description: "Fixture result", icon_url: icon, downloads: 1234 }];
    throw new Error(`Unexpected command: ${command}`);
  });
  let mods = $state<AutoMod[]>(Array.from({ length: 20 }, (_, index) => ({
    project_id: `mod-${index}`, name: `Recommendation ${index + 1}`, description: `Fixture description ${index + 1}`,
    icon_url: index === 0 ? null : icon, enabled: true, tags: index % 2 ? ["beta"] : [], loaders: ["fabric"], install_rank: 1, keep_priority: 50,
  })));
  let closed = $state(false);
</script>

<div class="flex flex-col" style="width:750px;height:470px" data-closed={closed} data-icon-calls={calls.filter((command) => command === "fetch_cached_icon").length} data-metadata-requests={metadataRequests} data-saves={saves}>
  <RecModsPanel autoMods={mods} onSave={(next) => { saves++; mods = next; }} onClose={() => closed = true} />
</div>
