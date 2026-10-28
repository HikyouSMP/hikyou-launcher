<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { Trash2, Upload } from "@lucide/svelte";
  import { createTranslation } from "../../lib/i18n.svelte";
  import type { ProfileContentItem } from "../../types";
  import { formatBytes, readFileBytes } from "../../utils/fileImport";
  import { EmptyLibraryState } from "./LibraryShell";

  let { profileId, kind }: { profileId: string; kind: "shader" | "resource_pack" } = $props();
  const { t } = createTranslation();
  let items = $state<ProfileContentItem[]>([]);
  let error = $state<string | null>(null);
  let input: HTMLInputElement;
  let request = 0;

  async function reload() {
    const current = ++request;
    try {
      const result = await invoke<ProfileContentItem[]>("list_profile_content", { profileId, kind });
      if (current !== request) return;
      items = result;
      error = null;
    } catch (reason) { if (current === request) error = String(reason); }
  }
  $effect(() => { void reload(); return () => { request++; }; });

  async function importFile(file: File) {
    try {
      await invoke("import_profile_content", { profileId, kind, filename: file.name, bytes: await readFileBytes(file, 256 * 1024 * 1024) });
      await reload();
    } catch (reason) { error = String(reason); }
  }
  async function remove(name: string) {
    try { await invoke("remove_profile_content", { profileId, kind, name }); await reload(); }
    catch (reason) { error = String(reason); }
  }
</script>

<div class="p-3">
  <div class="library-surface-heading">
    <div>
      <div class="text-[14px] font-medium text-t1">{kind === "shader" ? t("library.shaders") : t("library.resource_packs")}</div>
      <div class="text-[10px] text-t3">{t("library.item_count", { count: items.length })}</div>
    </div>
    <div class="flex items-center gap-1.5">
      <button class="icon-btn" onclick={() => input?.click()} title={t("library.import")}><Upload size={14} /></button>
      <input bind:this={input} type="file" accept=".zip,application/zip" class="hidden" onchange={(event) => {
        const file = event.currentTarget.files?.[0]; event.currentTarget.value = "";
        if (file) void importFile(file);
      }} />
    </div>
  </div>
  {#if error}<div class="mb-2 rounded-md bg-danger-bg px-2.5 py-2 text-[11px] text-danger">{error}</div>{/if}
  {#if items.length === 0}
    <EmptyLibraryState>{t("library.no_content")}</EmptyLibraryState>
  {:else}
    <div class="space-y-1">
      {#each items as item (item.name)}
        <div class="library-row">
          <div class="min-w-0 flex-1">
            <div class="truncate text-[12px] text-t1">{item.name}</div>
            <div class="text-[10px] text-t3">{item.directory ? t("library.folder") : formatBytes(item.sizeBytes)}</div>
          </div>
          <button class="icon-btn danger" title={t("common.delete")} onclick={() => remove(item.name)}><Trash2 size={13} /></button>
        </div>
      {/each}
    </div>
  {/if}
</div>
