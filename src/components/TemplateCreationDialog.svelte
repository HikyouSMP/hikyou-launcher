<script lang="ts">
  import { createTranslation } from '../lib/i18n.svelte';
  import type { TemplatePreview } from '../modTemplate';
  import ModalBackdrop from './ModalBackdrop.svelte';
  import { onMount } from 'svelte';
  let { review, answer }: { review: { preview: TemplatePreview; mcVersion: string; loader: string }; answer: (ids: string[] | null) => void } = $props();
  const { t } = createTranslation();
  const available = $derived(review.preview.items.filter(i=>i.status==='available'));
  let selected = $state<string[]>([]);
  let dialog: HTMLDivElement;
  onMount(() => { selected=available.map(i=>i.projectId); const previous=document.activeElement; dialog.focus(); return()=>{if(previous instanceof HTMLElement && previous.isConnected)previous.focus();}; });
  function keydown(event:KeyboardEvent){
    if(event.key==='Escape'){event.stopPropagation();answer(null);}
    if(event.key==='Tab'){
      const controls=Array.from(dialog.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'));
      const first=controls[0],last=controls[controls.length-1];
      if(event.shiftKey && (document.activeElement===first||document.activeElement===dialog)){event.preventDefault();last?.focus();}
      else if(!event.shiftKey && document.activeElement===last){event.preventDefault();first?.focus();}
    }
  }
</script>
<div class="fixed inset-0 z-[200] flex items-center justify-center" role="presentation">
  <ModalBackdrop fixed onClick={()=>answer(null)}/>
  <div bind:this={dialog} role="dialog" aria-modal="true" aria-label={t('mod_template.creation_title')} tabindex="-1" class="template-creation" onkeydown={keydown}>
    <h2>{t('mod_template.creation_title')}</h2><p>{review.mcVersion} · {review.loader}</p><p>{t('mod_template.creation_summary',{count:available.length,total:review.preview.items.length})}</p>
    <ul class="sb">{#each review.preview.items as item(item.projectId)}<li><span>{item.name}</span>{#if item.status==='available'}<button class="modal-btn" aria-pressed={selected.includes(item.projectId)} onclick={()=>selected=selected.includes(item.projectId)?selected.filter(id=>id!==item.projectId):[...selected,item.projectId]}>{t(selected.includes(item.projectId)?'plus_setup.selected':'plus_setup.excluded')}</button>{:else}<span class="text-t2">{t('mod_template.unavailable')}</span>{/if}</li>{/each}</ul>
    <p>{t('mod_template.creation_scope')}</p><div class="flex flex-wrap gap-2"><button class="modal-btn" onclick={()=>answer(selected)}>{t('mod_template.use_available')}</button><button class="modal-btn" onclick={()=>answer([])}>{t('mod_template.without')}</button><button class="modal-btn" onclick={()=>answer(null)}>{t('common.cancel')}</button></div>
  </div>
</div>
<style>.template-creation{position:relative;z-index:1;width:min(540px,calc(100vw - 32px));max-height:calc(100vh - 40px);overflow-y:auto;padding:22px;border-radius:8px;background:var(--color-surface);color:var(--color-t1);box-shadow:0 12px 40px #0006;font-size:13px}.template-creation h2{font-size:16px;margin:0}.template-creation p{line-height:1.6;color:var(--color-t2)}ul{list-style:none;padding:0;max-height:220px;overflow:auto}li{display:flex;justify-content:space-between;gap:16px;padding:8px 0;overflow-wrap:anywhere}</style>
