<script lang="ts">
  import { mockIPC } from "@tauri-apps/api/mocks";
  import ModListView from "../../src/components/ModListView.svelte";
  import type { TemplateEntry } from "../../src/modTemplate";

  const icon = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aC1sAAAAASUVORK5CYII=";
  const calls: string[] = [];
  let metadataRequests = $state(0);
  let saves = $state(0);
  mockIPC((command, args) => {
    calls.push(command);
    if(command==='get_mod_template')return {revision:String(saves),entries:mods,folders:[],legacyEntries:[],migrationRequired:false};
    if(command==='save_mod_template' && args && typeof args==='object' && 'entries' in args){saves++;mods=args.entries as TemplateEntry[];return {revision:String(saves),entries:mods,folders:[],legacyEntries:[],migrationRequired:false};}
    if (command === "fetch_cached_icon") {
      if (args && typeof args === "object" && "url" in args && "projectId" in args && args.url === null && args.projectId === "mod-0") metadataRequests++;
      return { data_url: icon, warning: null };
    }
    if (command === "search_modrinth") return [{ project_id: "search-result", title: "Search result", description: "Fixture result", icon_url: icon, downloads: 1234 }];
    throw new Error(`Unexpected command: ${command}`);
  });
  let mods = $state<TemplateEntry[]>(Array.from({ length: 20 }, (_, index) => ({
    projectId: `mod-${index}`, name: `Recommendation ${index + 1}`,
    iconUrl: index === 0 ? null : icon, loaders: null, folderId: null,
  })));
  let closed = $state(false);
  const entries=$derived(mods.map(mod=>({id:mod.projectId,projectId:mod.projectId,name:mod.name,icon:mod.iconUrl,enabled:true,canDelete:true})));
  function remove(id:string){saves++;mods=mods.filter(mod=>mod.projectId!==id);}
</script>

<div class="flex flex-col" style="width:750px;height:470px" data-closed={closed} data-icon-calls={calls.filter((command) => command === "fetch_cached_icon").length} data-metadata-requests={metadataRequests} data-saves={saves}>
  <ModListView onClose={() => closed = true} {entries} loading={false} searchPlaceholder="Search Mods" loader="fabric" mcVersion="" onEntryDelete={remove} searchState={()=>'idle'} onInstall={async()=>{}}/>
</div>
