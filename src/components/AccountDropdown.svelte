<script lang="ts">
  import { ChevronUp, User, LogOut, Settings, Plus } from "@lucide/svelte";
  import { fly } from "svelte/transition";
  import { css } from "../lib/dom";
  import { createTranslation } from "../lib/i18n.svelte";
  import { C } from "../theme";
  import type { StoredAuth } from "../types";
  import AccountAvatar from "./AccountAvatar.svelte";

  let { currentAccount, onLogout, onAddAccount, onOpenSettings }: {
    currentAccount: StoredAuth | null;
    onLogout: () => void;
    onAddAccount: () => void;
    onOpenSettings: () => void;
  } = $props();
  const { t } = createTranslation();
  let isOpen = $state(false);
  let hoveredItem = $state<string | null>(null);
  const skinName = $derived(currentAccount?.uuid ?? currentAccount?.username ?? null);
  const menuItemStyle = (key: string, danger = false) => css({
    width: "100%", display: "flex", alignItems: "center", gap: 10, padding: "7px 10px",
    borderRadius: C.r,
    background: hoveredItem === key ? danger ? C.dangerBg : "rgba(255,255,255,.075)" : "transparent",
    border: "none", color: hoveredItem === key ? danger ? C.danger : C.t1 : danger ? "rgba(160,40,65,.7)" : C.t2,
    fontSize: 13, cursor: "pointer", transition: "background .1s, color .1s", textAlign: "left",
  });
</script>

{#if currentAccount}
  <div class="fixed bottom-4 right-4 z-[60]">
    {#if isOpen}
      <div class="fixed inset-0 z-[-1]" onclick={() => { isOpen = false; }} role="presentation"></div>
      <div class="account-panel absolute w-[220px] rounded-[8px] overflow-hidden" style="bottom:calc(100% + 8px);right:0"
        transition:fly={{ y: 8, duration: 150 }}>
        <div class="account-panel-header px-3 py-[10px]">
          <div class="text-[10px] font-semibold text-t3 uppercase tracking-[0.12em] mb-1.5">{t("account_dropdown.active_account")}</div>
          <div class="flex items-center gap-[10px]">
            <div class="account-avatar w-8 h-8 rounded-[6px] flex items-center justify-center text-t3 overflow-hidden shrink-0">
              <AccountAvatar skinId={skinName} size={32}>{#snippet fallback()}<User size={16} />{/snippet}</AccountAvatar>
            </div>
            <div class="min-w-0">
              <div class="text-[13px] font-semibold text-t1 overflow-hidden text-ellipsis whitespace-nowrap">{currentAccount.username ?? t("account_dropdown.unknown_user")}</div>
              <div class="text-[10px] text-t3">{t("account_dropdown.minecraft_premium")}</div>
            </div>
          </div>
        </div>
        <div class="p-1">
          <button style={menuItemStyle("settings")} onmouseenter={() => { hoveredItem = "settings"; }} onmouseleave={() => { hoveredItem = null; }}
            onclick={() => { onOpenSettings(); isOpen = false; }}><Settings size={14} />{t("account_dropdown.settings")}</button>
          <button style={menuItemStyle("add")} onmouseenter={() => { hoveredItem = "add"; }} onmouseleave={() => { hoveredItem = null; }}
            onclick={() => { onAddAccount(); isOpen = false; }}><Plus size={14} />{t("account_dropdown.add_account")}</button>
          <div class="account-separator h-px my-1"></div>
          <button style={menuItemStyle("logout", true)} onmouseenter={() => { hoveredItem = "logout"; }} onmouseleave={() => { hoveredItem = null; }}
            onclick={() => { onLogout(); isOpen = false; }}><LogOut size={14} />{t("account_dropdown.logout")}</button>
        </div>
      </div>
    {/if}
    <button onclick={() => { isOpen = !isOpen; }} aria-expanded={isOpen}
      class="account-trigger btn-phys flex items-center gap-2 px-3 py-1.5 rounded-[20px] cursor-pointer transition-colors duration-100"
      style={css({ background: "rgba(46,45,43,.55)", backdropFilter: "blur(18px) saturate(150%)", WebkitBackdropFilter: "blur(18px) saturate(150%)" })}
      onmouseenter={(event) => { event.currentTarget.style.background = "rgba(255,255,255,.09)"; }}
      onmouseleave={(event) => { event.currentTarget.style.background = "rgba(46,45,43,.55)"; }}>
      <div class="account-avatar w-6 h-6 rounded-[6px] flex items-center justify-center text-t3 overflow-hidden shrink-0">
        <AccountAvatar skinId={skinName} size={24}>{#snippet fallback()}<User size={12} />{/snippet}</AccountAvatar>
      </div>
      <span class="text-[13px] font-medium text-t1">{currentAccount.username ?? t("account_dropdown.unknown_user")}</span>
      <ChevronUp size={14} color={C.t3} style={css({ transition: "transform 0.2s", transform: isOpen ? "rotate(180deg)" : "rotate(0deg)" })} />
    </button>
  </div>
{/if}
