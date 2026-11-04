import { invoke } from '@tauri-apps/api/core';
import { untrack } from 'svelte';
import { modLifecycleFailure, type ModLifecycleFailure } from '../modLifecycleFailure';
import type { ModTemplate, TemplateEntry, TemplateFolder } from '../modTemplate';
export function useModTemplate() {
  let snapshot = $state<ModTemplate | null>(null); let busy = $state(false); let failure = $state<ModLifecycleFailure | null>(null);
  let epoch = 0; let disposed = false;
  async function load() {
    if (busy) return; const current = ++epoch; busy = true;
    try { const value = await invoke<ModTemplate>('get_mod_template'); if (!disposed && current === epoch) { snapshot = value; failure = null; } }
    catch (error) { if (!disposed && current === epoch) { snapshot=null; failure = modLifecycleFailure(error, 'get_mod_template'); } }
    finally { if (!disposed && current === epoch) busy = false; }
  }
  $effect(() => { disposed = false; untrack(() => { void load(); }); return () => { disposed = true; epoch++; }; });
  async function save(entries: TemplateEntry[], folders: TemplateFolder[]) {
    if (busy || !snapshot || disposed) return;
    const revision = snapshot.revision; const current = ++epoch; busy = true;
    try { const value = await invoke<ModTemplate>('save_mod_template', { expectedRevision: revision, entries, folders }); if (!disposed && current === epoch) { snapshot = value; failure = null; } }
    catch (error) { if (!disposed && current === epoch) { failure = modLifecycleFailure(error, 'save_mod_template'); try { const value = await invoke<ModTemplate>('get_mod_template'); if (!disposed && current === epoch) snapshot = value; } catch { if (!disposed && current === epoch) snapshot = null; } } }
    finally { if (!disposed && current === epoch) busy = false; }
  }
  return { get snapshot() { return snapshot; }, get busy() { return busy; }, get failure() { return failure; }, load, save };
}
