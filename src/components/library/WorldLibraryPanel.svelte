<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ArrowLeft, Lock, Trash2, Upload } from "@lucide/svelte";
  import { createTranslation } from "../../lib/i18n.svelte";
  import type { DatapackItem, WorldSummary } from "../../types";
  import { formatBytes, readFileBytes } from "../../utils/fileImport";
  import { EmptyLibraryState } from "./LibraryShell";

  let { profileId }: { profileId: string } = $props();
  const { t } = createTranslation();
  let worlds = $state<WorldSummary[]>([]);
  let selected = $state<WorldSummary | null>(null);
  let datapacks = $state<DatapackItem[]>([]);
  let error = $state<string | null>(null);
  let worldInput = $state<HTMLInputElement>();
  let datapackInput = $state<HTMLInputElement>();
  let worldRequest = 0;
  let datapackRequest = 0;

  async function loadWorlds() {
    const current = ++worldRequest;
    try {
      const result = await invoke<WorldSummary[]>("list_worlds", { profileId });
      if (current !== worldRequest) return;
      worlds = result; error = null;
    } catch (reason) { if (current === worldRequest) error = String(reason); }
  }
  async function loadDatapacks() {
    const current = ++datapackRequest;
    if (!selected) return;
    try {
      const result = await invoke<DatapackItem[]>("list_datapacks", { profileId, worldId: selected.id });
      if (current !== datapackRequest) return;
      datapacks = result; error = null;
    } catch (reason) { if (current === datapackRequest) error = String(reason); }
  }
  $effect(() => { void loadWorlds(); return () => { worldRequest++; }; });
  $effect(() => { void loadDatapacks(); return () => { datapackRequest++; }; });

  async function importWorld(file: File) {
    try {
      await invoke("import_world", { profileId, filename: file.name, bytes: await readFileBytes(file, 512 * 1024 * 1024) });
      await loadWorlds();
    } catch (reason) { error = String(reason); }
  }
  async function importDatapack(file: File) {
    if (!selected) return;
    try {
      await invoke("import_datapack", { profileId, worldId: selected.id, filename: file.name, bytes: await readFileBytes(file, 256 * 1024 * 1024) });
      await loadDatapacks();
    } catch (reason) { error = String(reason); }
  }
  async function removeDatapack(name: string) {
    if (!selected) return;
    try { await invoke("remove_datapack", { profileId, worldId: selected.id, name }); await loadDatapacks(); }
    catch (reason) { error = String(reason); }
  }
</script>

{#if selected}
  <div class="p-3">
    <div class="library-surface-heading">
      <div class="flex min-w-0 items-center gap-2">
        <button class="icon-btn" onclick={() => selected = null} title={t("common.back")}><ArrowLeft size={14} /></button>
        <div class="min-w-0 flex-1">
          <div class="truncate text-[13px] font-medium text-t1">{selected.name}</div>
          <div class="text-[10px] text-t3">{[selected.versionName, selected.gameMode, selected.hardcore ? "Hardcore" : null].filter(Boolean).join(" · ")}</div>
        </div>
      </div>
      <button class="icon-btn" disabled={selected.locked} onclick={() => datapackInput?.click()} title={t("library.import_datapack")}><Upload size={14} /></button>
      <input bind:this={datapackInput} type="file" accept=".zip,application/zip" class="hidden" onchange={(event) => {
        const file = event.currentTarget.files?.[0]; event.currentTarget.value = ""; if (file) void importDatapack(file);
      }} />
    </div>
    {#if selected.locked}
      <div class="mb-2 flex items-center gap-1.5 rounded-md bg-warning-bg px-2.5 py-2 text-[11px] text-warning"><Lock size={12} /> {t("library.world_in_use")}</div>
    {/if}
    {#if error}<div class="mb-2 rounded-md bg-danger-bg px-2.5 py-2 text-[11px] text-danger">{error}</div>{/if}
    <div class="mb-2 px-1 text-[11px] text-t2">{t("library.datapacks")}</div>
    {#if datapacks.length === 0}<EmptyLibraryState>{t("library.no_datapacks")}</EmptyLibraryState>
    {:else}
      <div class="space-y-1">
        {#each datapacks as item (item.name)}
          <div class="library-row">
            <div class="min-w-0 flex-1">
              <div class="truncate text-[12px] text-t1">{item.name}</div>
              <div class="text-[10px] text-t3">{item.directory ? t("library.folder") : formatBytes(item.sizeBytes)}</div>
            </div>
            <button class="icon-btn danger" disabled={selected.locked} title={t("common.delete")} onclick={() => removeDatapack(item.name)}><Trash2 size={13} /></button>
          </div>
        {/each}
      </div>
    {/if}
  </div>
{:else}
  <div class="p-3">
    <div class="library-surface-heading">
      <div>
        <div class="text-[14px] font-medium text-t1">{t("library.worlds")}</div>
        <div class="text-[10px] text-t3">{t("library.item_count", { count: worlds.length })}</div>
      </div>
      <button class="icon-btn" onclick={() => worldInput?.click()} title={t("library.import_world")}><Upload size={14} /></button>
      <input bind:this={worldInput} type="file" accept=".zip,application/zip" class="hidden" onchange={(event) => {
        const file = event.currentTarget.files?.[0]; event.currentTarget.value = ""; if (file) void importWorld(file);
      }} />
    </div>
    {#if error}<div class="mb-2 rounded-md bg-danger-bg px-2.5 py-2 text-[11px] text-danger">{error}</div>{/if}
    {#if worlds.length === 0}<EmptyLibraryState>{t("library.no_worlds")}</EmptyLibraryState>
    {:else}
      <div class="space-y-1">
        {#each worlds as world (world.id)}
          <button class="library-row w-full text-left" onclick={() => { datapacks = []; selected = world; }}>
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-1.5">
                <span class="truncate text-[12px] text-t1">{world.name}</span>
                {#if world.locked}<Lock size={10} class="shrink-0 text-warning" />{/if}
              </div>
              <div class="truncate text-[10px] text-t3">{[world.versionName, world.gameMode, world.lastPlayed ? new Date(world.lastPlayed).toLocaleDateString() : null].filter(Boolean).join(" · ")}</div>
            </div>
          </button>
        {/each}
      </div>
    {/if}
  </div>
{/if}
