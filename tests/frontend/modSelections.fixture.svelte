<script lang="ts">
  import { mockIPC } from '@tauri-apps/api/mocks';
  import ModTemplatePanel from '../../src/components/ModTemplatePanel.svelte';
  import PlusPlaySetup from '../../src/components/PlusPlaySetup.svelte';
  import TemplateCreationDialog from '../../src/components/TemplateCreationDialog.svelte';
  import i18n from '../../src/i18n';
  import type { ModTemplate, TemplateEntry, TemplateFolder } from '../../src/modTemplate';
  let view=$state('template'),calls=$state(0),confirmed=$state(0),failedNext=$state(false),answer=$state<string|null>(null);
  let template:ModTemplate={revision:'r0',entries:[{projectId:'sodium',name:'Sodium',iconUrl:null,loaders:null,folderId:null},{projectId:'chat',name:'Chat Heads',iconUrl:null,loaders:['fabric'],folderId:'visual'}],folders:[{id:'visual',parentId:null,name:'Visual'}],migrationRequired:false,legacyEntries:[]};
  let setupRequired=true;
  const row=(id:string,name:string,supportsFabric:boolean,desired='required')=>({key:`modrinth:${id}`,projectId:id,objectId:null,name,iconUrl:null,desired,source:'user',active:null,reason:'pending',canEnable:false,canExclude:true,canInherit:false,supportsFabric});
  mockIPC((command,args)=>{
    calls++;
    const data=JSON.parse(JSON.stringify(args??{})) as Record<string,unknown>;
    if(command==='fetch_cached_icon')return {data_url:null,warning:null};
    if(command==='get_mod_template')return structuredClone(template);
    if(command==='save_mod_template'){
      if(failedNext){failedNext=false;throw new Error('Permission denied');}
      if(data.expectedRevision!==template.revision)throw new Error('template_changed_refresh_required');
      template={...template,revision:`r${calls}`,entries:structuredClone(data.entries as TemplateEntry[]),folders:structuredClone(data.folders as TemplateFolder[])};
      return structuredClone(template);
    }
    if(command==='search_modrinth')return [{project_id:'newmod',title:'New Mod',icon_url:null,downloads:12,description:'Fixture'}];
    if(command==='get_plus_play_setup')return {profileId:'smart:latest-plus',revision:'setup-v1',required:setupRequired,rows:[row('sodium','Sodium',true,'excluded'),row('chat','Chat Heads',true),row('exclusive','Other Loader Mod',false)],warnings:[]};
    if(command==='confirm_plus_play_setup'){
      if(data.expectedRevision!=='setup-v1'||!Array.isArray(data.includeKeys)||data.includeKeys.includes('modrinth:exclusive'))throw new Error('invalid fixture selection');
      confirmed++;setupRequired=false;return null;
    }
    throw new Error(`Unexpected fixture command: ${command}`);
  });
</script>
<div class="fixture" data-calls={calls} data-confirmed={confirmed} data-answer={answer??''}>
  <nav><button onclick={()=>view='template'}>Template</button><button onclick={()=>view='setup'}>Plus setup</button><button onclick={()=>view='creation'}>Create profile</button><button onclick={()=>failedNext=true}>Fail next save</button><button onclick={()=>i18n.changeLanguage(i18n.language==='ja'?'en':'ja')}>Language</button></nav>
  {#if view==='template'}<ModTemplatePanel onClose={()=>view='closed'}/>{/if}
  {#if view==='setup'}<PlusPlaySetup profileId="smart:latest-plus" onComplete={()=>{}}/>{/if}
  {#if view==='creation'}<TemplateCreationDialog review={{mcVersion:'1.21.1',loader:'fabric',preview:{revision:'r1',token:'fixture',items:[{projectId:'sodium',name:'Sodium',iconUrl:null,status:'available',reason:null},{projectId:'later',name:'Later Mod',iconUrl:null,status:'unavailable',reason:'no_target_release'}]}}} answer={(ids)=>{answer=JSON.stringify(ids);view='closed';}}/>{/if}
</div>
<style>.fixture{height:100vh;display:flex;flex-direction:column;background:#1c1d1b;color:#e5e6e3}nav{display:flex;gap:8px;flex-wrap:wrap;padding:10px;font-size:12px}nav button{padding:4px 8px;background:#343532;border:0;color:inherit}</style>
