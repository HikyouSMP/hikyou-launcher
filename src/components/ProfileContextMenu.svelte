<script module lang="ts">
  export interface ProfileContextMenuState { profileId: string; x: number; y: number }
</script>

<script lang="ts">
  import { FolderCog, Play, SlidersHorizontal, Square, Trash2 } from "@lucide/svelte";
  import { createTranslation } from "../lib/i18n.svelte";
  import { css } from "../lib/dom";
  import type { Profile, StoredAuth } from "../types";
  let { menu, profiles, savedAuth, isProfileBusy, onClose, onLaunch, onStop, onLogin, onEditSettings, onManage, onDelete }: {
    menu: ProfileContextMenuState; profiles: Profile[]; savedAuth: StoredAuth | null; isProfileBusy: (profileId: string) => boolean;
    onClose: () => void; onLaunch: (profile: Profile) => void; onStop: (profileId: string) => void; onLogin: () => void;
    onEditSettings: (profileId: string) => void; onManage: (profileId: string) => void; onDelete: (profileId: string) => void;
  } = $props();
  const { t } = createTranslation();
  let profile = $derived(profiles.find((item) => item.id === menu.profileId));
  let running = $derived(isProfileBusy(menu.profileId));
  let smart = $derived(profile?.kind === "smart");
  const menuW = 168;
  let menuH = $derived(smart ? 104 : 128);
  let x = $derived(Math.min(menu.x, 750 - menuW - 8));
  let y = $derived(Math.min(menu.y, 470 - menuH - 8));
  function activate(action: (target: Profile) => void) {
    const target = profile;
    if (!target) return;
    onClose();
    action(target);
  }
</script>

{#if profile}
  <div class="absolute inset-0 z-58" onmousedown={onClose} role="presentation"></div>
  <div class="absolute z-60 menu-panel" style={css({ left: x, top: y, width: menuW })} onmousedown={(event) => event.stopPropagation()} role="menu" tabindex="-1">
    {#if running}
      <div class="menu-item danger" role="menuitem" tabindex="0" onkeydown={(e) => { if (e.key === "Enter") activate((target) => onStop(target.id)); }} onclick={() => activate((target) => onStop(target.id))}><Square size={13} />{t("profile.stop")}</div>
    {:else}
      <div class="menu-item" role="menuitem" tabindex="0" onkeydown={(e) => { if (e.key === "Enter") activate((target) => savedAuth ? onLaunch(target) : onLogin()); }} onclick={() => activate((target) => savedAuth ? onLaunch(target) : onLogin())}><Play size={13} />{t("profile.launch")}</div>
    {/if}
    <div class="menu-item" role="menuitem" tabindex="0" onkeydown={(e) => { if (e.key === "Enter") activate((target) => onEditSettings(target.id)); }} onclick={() => activate((target) => onEditSettings(target.id))}><SlidersHorizontal size={13} />{t("profile.edit_settings")}</div>
    <div class="menu-item" role="menuitem" tabindex="0" onkeydown={(e) => { if (e.key === "Enter") activate((target) => onManage(target.id)); }} onclick={() => activate((target) => onManage(target.id))}><FolderCog size={13} />{t("library.manage")}</div>
    {#if !smart}
      <div class="h-px my-1 bg-b1"></div>
      <div class={`menu-item danger ${running ? "disabled" : ""}`} role="menuitem" aria-disabled={running} tabindex={running ? -1 : 0}
        onkeydown={(e) => { if (e.key === "Enter" && !running) activate((target) => onDelete(target.id)); }}
        onclick={() => { if (!running) activate((target) => onDelete(target.id)); }}><Trash2 size={13} />{running ? t("profile.delete_busy") : t("common.delete")}</div>
    {/if}
  </div>
{/if}
