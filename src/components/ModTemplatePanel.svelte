<script lang="ts">
  import { ArrowLeft, Search, Plus, Trash2 } from '@lucide/svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { createTranslation } from '../lib/i18n.svelte';
  import { useModTemplate } from '../hooks/useModTemplate.svelte';
  import { dissolveFolder, folderLabel, type TemplateEntry } from '../modTemplate';
  import type { ModSearchResult } from '../types';
  import ModIcon from './ModIcon.svelte';
  import ModFailureDetails from './ModFailureDetails.svelte';
  let { onClose }: { onClose: () => void } = $props();
  const { t } = createTranslation(); const model = useModTemplate();
  let query = $state(''); let filter = $state(''); let folder = $state(''); let selected = $state<string | null>(null);
  let results = $state<ModSearchResult[]>([]); let searching = $state(false); let searchFailed = $state(false); let searchId = 0;
  let folderName = $state('');
  const entries = $derived(model.snapshot?.entries ?? []); const folders = $derived(model.snapshot?.folders ?? []);
  const visible = $derived(entries.filter(entry => (!filter || !entry.loaders || entry.loaders.includes(filter)) && (!folder || entry.folderId === folder) && (!query.trim() || entry.name.toLowerCase().includes(query.trim().toLowerCase()))));
  const current = $derived(entries.find(entry => entry.projectId === selected));
  async function search() {
    if (!query.trim()) { results = []; return; } const id = ++searchId; searching = true; searchFailed = false;
    try { const value = await invoke<ModSearchResult[]>('search_modrinth', { query: query.trim(), mcVersion: '', loader: filter }); if (id === searchId) results = value; }
    catch { if (id === searchId) searchFailed = true; }
    finally { if (id === searchId) searching = false; }
  }
  function add(mod: ModSearchResult) {
    if (entries.some(entry => entry.projectId === mod.project_id)) { selected = mod.project_id; return; }
    selected = mod.project_id;
    void model.save([...entries, { projectId: mod.project_id, name: mod.title, iconUrl: mod.icon_url ?? null, loaders: null, folderId: folder || null }], folders);
  }
  function update(patch: Partial<TemplateEntry>) { if (current) void model.save(entries.map(entry => entry.projectId === current.projectId ? { ...entry, ...patch } : entry), folders); }
  function addFolder() { if (!folderName.trim()) return; const next = { id: crypto.randomUUID(), parentId: folder || null, name: folderName.trim() }; folderName = ''; void model.save(entries, [...folders, next]); }
</script>

<section class="template-editor">
  <header><button class="icon-btn" title={t('common.back')} onclick={onClose}><ArrowLeft size={18}/></button><div><h2>{t('mod_template.title')}</h2><p>{t('mod_template.scope')}</p></div></header>
  {#if model.failure}<div role="alert" class="template-alert">{t('mod_template.failed')} <button class="modal-btn" onclick={() => model.load()} disabled={model.busy}>{t('common.retry')}</button><ModFailureDetails failures={[model.failure]}/></div>{/if}
  {#if model.snapshot?.migrationRequired && model.snapshot.legacyEntries.length}<div class="template-alert"><span>{t('mod_template.legacy')}</span><button class="modal-btn" disabled={model.busy} onclick={() => model.save(model.snapshot?.legacyEntries ?? [], [])}>{t('mod_template.copy_legacy')}</button></div>{/if}
  <div class="template-toolbar">
    <form onsubmit={(event) => {event.preventDefault();void search();}}><input aria-label={t('mods.search_placeholder')} bind:value={query} placeholder={t('mods.search_placeholder')}/><button class="icon-btn" title={t('mod_template.search')} disabled={searching}><Search size={17}/></button></form>
    <select aria-label={t('mod_template.loader_scope')} bind:value={filter}><option value="">{t('mod_template.all_loaders')}</option>{#each ['fabric','neoforge','quilt','forge'] as loader}<option value={loader}>{loader === 'neoforge' ? 'NeoForge' : loader[0].toUpperCase()+loader.slice(1)}</option>{/each}</select>
    <select aria-label={t('mod_template.folder')} bind:value={folder}><option value="">{t('mod_template.all_folders')}</option>{#each folders as item(item.id)}<option value={item.id}>{folderLabel(folders,item.id)}</option>{/each}</select>
  </div>
  <div class="template-columns">
    <div class="template-collection sb" aria-busy={model.busy}>
      {#if !model.snapshot && !model.failure}<p>{t('common.fetching')}</p>{/if}
      {#each visible as entry(entry.projectId)}<button class:chosen={selected === entry.projectId} class="template-row" onclick={() => selected=entry.projectId}><ModIcon name={entry.name} url={entry.iconUrl} projectId={entry.projectId} size={32}/><span>{entry.name}<small>{entry.loaders?.join(' / ') ?? t('mod_template.all_loaders')}{entry.folderId ? ` · ${folderLabel(folders,entry.folderId)}` : ''}</small></span></button>{/each}
      {#if model.snapshot && !entries.length}<p>{t('mod_template.empty')}</p>{/if}
      {#if searching}<p>{t('common.fetching')}</p>{/if}{#if searchFailed}<p role="alert">{t('mod_template.search_failed')}</p>{/if}
      {#if results.length}<h3>{t('mod_template.results')}</h3>{/if}
      {#each results as mod(mod.project_id)}<div class="template-result"><ModIcon name={mod.title} url={mod.icon_url} projectId={mod.project_id} size={28}/><span>{mod.title}</span><button class="icon-btn" title={t('mod_template.add')} disabled={model.busy || entries.some(e => e.projectId === mod.project_id)} onclick={() => add(mod)}><Plus size={17}/></button></div>{/each}
    </div>
    <aside class="template-details sb">
      {#if current}
        <ModIcon name={current.name} url={current.iconUrl} projectId={current.projectId} size={48}/><h3>{current.name}</h3>
        <label>{t('mod_template.apply_on')}<select aria-label={t('mod_template.apply_on')} disabled={model.busy} bind:value={()=>current?.loaders?.[0] ?? '', value=>update({loaders:value ? [value] : null})}><option value="">{t('mod_template.all_loaders')}</option>{#each ['fabric','neoforge','quilt','forge'] as loader}<option value={loader}>{loader}</option>{/each}</select></label>
        <label>{t('mod_template.folder')}<select disabled={model.busy} bind:value={()=>current?.folderId ?? '', value=>update({folderId:value || null})}><option value="">{t('mod_template.unfiled')}</option>{#each folders as item(item.id)}<option value={item.id}>{folderLabel(folders,item.id)}</option>{/each}</select></label>
        <button class="modal-btn" disabled={model.busy} onclick={() => model.save(entries.filter(entry => entry.projectId !== current.projectId),folders)}><Trash2 size={14}/>{t('mod_template.remove')}</button>
      {:else}<p>{t('mod_template.select')}</p>{/if}
      <h3>{t('mod_template.organize')}</h3>
      <form onsubmit={(event) => {event.preventDefault();addFolder();}}><input aria-label={t('mod_template.folder_name')} bind:value={folderName} placeholder={t('mod_template.folder_name')} maxlength={256}/><button class="icon-btn" title={t('mod_template.new_folder')} disabled={model.busy || !folderName.trim()}><Plus size={17}/></button></form>
      {#if folder}<button class="modal-btn" disabled={model.busy} onclick={() => {if(model.snapshot){const value=dissolveFolder(model.snapshot,folder);folder='';void model.save(value.entries,value.folders);}}}>{t('mod_template.dissolve')}</button>{/if}
    </aside>
  </div>
</section>

<style>
  .template-editor{display:flex;flex-direction:column;height:100%;min-height:0;color:var(--color-t1);font-size:13px}
  header{display:flex;align-items:center;gap:12px;padding:16px}h2{font-size:16px;font-weight:600;margin:0}p{color:var(--color-t2);line-height:1.6;margin:5px 0}h3{font-size:13px;font-weight:500;margin:20px 0 10px}
  .template-toolbar{display:flex;gap:8px;padding:0 16px 14px;flex-wrap:wrap}.template-toolbar form{flex:1;min-width:180px}form{display:flex;gap:6px}input,select{background:var(--color-surface);border:0;border-radius:6px;color:inherit;padding:9px;font:inherit;min-width:0}input{width:100%}.template-columns{display:grid;grid-template-columns:minmax(0,1fr) minmax(200px,34%);flex:1;min-height:0}.template-collection{overflow-y:auto;padding:0 12px 16px}.template-details{padding:0 16px 16px;overflow-y:auto;background:rgba(255,255,255,.025)}
  .template-row{display:flex;width:100%;align-items:center;gap:10px;border:0;background:transparent;text-align:left;color:inherit;padding:10px;border-radius:6px;cursor:pointer;font:inherit}.template-row:hover,.chosen{background:var(--color-hover)}.template-row span{min-width:0;overflow-wrap:anywhere}small{display:block;color:var(--color-t2);font-size:11px;margin-top:4px}.template-result{display:flex;align-items:center;gap:10px;padding:8px}.template-result span{flex:1;min-width:0;overflow-wrap:anywhere}label{display:flex;flex-direction:column;gap:7px;margin:16px 0;color:var(--color-t2)}.template-alert{padding:10px 16px;font-size:12px;line-height:1.6}.template-details .modal-btn{display:inline-flex;align-items:center;height:auto;min-height:32px;white-space:normal;gap:6px}
  @media(max-width:560px){.template-columns{grid-template-columns:minmax(0,1fr) minmax(160px,42%)}.template-toolbar{padding:0 10px 10px}.template-details{padding:0 10px 12px}}
</style>
