<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { Check, Plus, Trash2, X } from "@lucide/svelte";
  import { tick } from "svelte";
  import { createTranslation } from "../../lib/i18n.svelte";
  import type { ServerSummary } from "../../types";
  import { EmptyLibraryState } from "./LibraryShell";

  let { profileId }: { profileId: string } = $props();
  const { t } = createTranslation();
  let servers = $state<ServerSummary[]>([]);
  let editing = $state<ServerSummary | "new" | null>(null);
  let name = $state("");
  let address = $state("");
  let error = $state<string | null>(null);
  let nameInput = $state<HTMLInputElement>();
  let request = 0;

  async function reload() {
    const current = ++request;
    try {
      const result = await invoke<ServerSummary[]>("list_servers", { profileId });
      if (current !== request) return;
      servers = result; error = null;
    } catch (reason) { if (current === request) error = String(reason); }
  }
  $effect(() => { void reload(); return () => { request++; }; });

  async function beginEdit(server: ServerSummary | "new") {
    editing = server; name = server === "new" ? "" : server.name; address = server === "new" ? "" : server.address;
    await tick(); nameInput?.focus();
  }
  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (!editing) return;
    try {
      await invoke("save_server", { profileId, key: editing === "new" ? null : editing.key, name, address });
      editing = null; await reload();
    } catch (reason) { error = String(reason); }
  }
  async function remove(key: string) {
    try { await invoke("remove_server", { profileId, key }); await reload(); }
    catch (reason) { error = String(reason); }
  }
</script>

<div class="p-3">
  <div class="library-surface-heading">
    <div>
      <div class="text-[14px] font-medium text-t1">{t("library.servers")}</div>
      <div class="text-[10px] text-t3">{t("library.item_count", { count: servers.length })}</div>
    </div>
    <button class="icon-btn" onclick={() => beginEdit("new")} title={t("library.add_server")}><Plus size={14} /></button>
  </div>
  {#if error}<div class="mb-2 rounded-md bg-danger-bg px-2.5 py-2 text-[11px] text-danger">{error}</div>{/if}
  {#if editing}
    <form class="mb-2 grid grid-cols-[1fr_1.25fr_auto] gap-1.5 rounded-md bg-surface-raised p-2" onsubmit={save}>
      <input bind:this={nameInput} class="library-input" bind:value={name} placeholder={t("library.server_name")} />
      <input class="library-input" bind:value={address} placeholder={t("library.server_address")} />
      <div class="flex gap-1">
        <button class="icon-btn" type="submit" title={t("common.save")}><Check size={13} /></button>
        <button class="icon-btn" type="button" onclick={() => editing = null} title={t("common.close")}><X size={13} /></button>
      </div>
    </form>
  {/if}
  {#if servers.length === 0}<EmptyLibraryState>{t("library.no_servers")}</EmptyLibraryState>
  {:else}
    <div class="space-y-1">
      {#each servers as server (server.key)}
        <div class="library-row">
          <button class="min-w-0 flex-1 text-left" onclick={() => beginEdit(server)}>
            <div class="truncate text-[12px] text-t1">{server.name}</div>
            <div class="truncate text-[10px] text-t3">{server.address}</div>
          </button>
          <button class="icon-btn danger" title={t("common.delete")} onclick={() => remove(server.key)}><Trash2 size={13} /></button>
        </div>
      {/each}
    </div>
  {/if}
</div>
