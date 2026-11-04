<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { untrack } from 'svelte';
  import { createTranslation } from '../lib/i18n.svelte';
  import type { ModChoiceRow } from '../modLifecycle';
  import ModFailureDetails from './ModFailureDetails.svelte';
  import { modLifecycleFailure, type ModLifecycleFailure } from '../modLifecycleFailure';
  let { profileId, onComplete }: { profileId: string; onComplete: () => void } = $props();
  type Setup = { profileId: string; revision: string; required: boolean; rows: (ModChoiceRow & {supportsFabric: boolean})[]; warnings: string[] };
  const { t } = createTranslation(); let setup = $state<Setup | null>(null); let busy = $state(false); let failure = $state<ModLifecycleFailure | null>(null); let open = $state(false); let selected = $state<string[]>([]); let sequence = 0;
  async function load(id = profileId) {
    const current = ++sequence; busy = true;
    try { const result = await invoke<Setup>('get_plus_play_setup', {profileId:id}); if(current===sequence && id===profileId) {setup=result;selected=result.rows.filter(r=>r.supportsFabric && r.desired==='required').map(r=>r.key);failure=null;} }
    catch(error){if(current===sequence)failure=modLifecycleFailure(error,'get_plus_play_setup');}
    finally{if(current===sequence)busy=false;}
  }
  $effect(()=>{const id=profileId;setup=null;open=false;if(id.startsWith('smart:'))untrack(()=>{void load(id);});return()=>{sequence++;};});
  async function confirm(){
    if(!setup||busy)return;const id=profileId;const revision=setup.revision;busy=true;
    try{await invoke('confirm_plus_play_setup',{profileId:id,expectedRevision:revision,includeKeys:selected});if(id===profileId){setup={...setup,required:false};failure=null;onComplete();}}
    catch(error){if(id===profileId)failure=modLifecycleFailure(error,'confirm_plus_play_setup');}
    finally{if(id===profileId)busy=false;}
  }
</script>
{#if failure}<div role="alert" class="px-3 py-2 text-xs text-danger">{t('plus_setup.failed')} <button class="modal-btn" disabled={busy} onclick={()=>load()}>{t('common.retry')}</button><ModFailureDetails failures={[failure]}/></div>{/if}
{#if setup?.required}
  <section class="px-3 py-3 text-xs text-t2" data-plus-setup>
    <p class="m-0 leading-relaxed">{t('plus_setup.explain')}</p>
    <button class="modal-btn mt-2" disabled={busy} onclick={()=>open=!open}>{t('plus_setup.review')}</button>
    {#if open}<div class="max-h-64 overflow-y-auto sb mt-3">
      {#each setup.rows as row(row.key)}<div class="flex items-center justify-between gap-3 py-2"><span class="min-w-0 break-words">{row.name}<small class="block text-t3">{row.supportsFabric ? t(`mod_lifecycle.reason_${row.reason}`) : t('plus_setup.no_fabric')}</small></span><button class="modal-btn shrink-0" aria-pressed={selected.includes(row.key)} disabled={busy||!row.supportsFabric} onclick={()=>{selected=selected.includes(row.key)?selected.filter(k=>k!==row.key):[...selected,row.key];}}>{t(selected.includes(row.key)?'plus_setup.selected':'plus_setup.excluded')}</button></div>{/each}
    </div><p class="text-t3 leading-relaxed">{t('plus_setup.scope')}</p><button class="modal-btn" disabled={busy} onclick={confirm}>{t('plus_setup.confirm')}</button>{/if}
  </section>
{/if}
