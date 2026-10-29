<script lang="ts">
  import { LogOut, UserPlus } from "@lucide/svelte";
  import { createTranslation } from "../lib/i18n.svelte";
  import { css } from "../lib/dom";
  import { C } from "../theme";
  import type { StoredAuth } from "../types";
  import AccountAvatar from "./AccountAvatar.svelte";

  let { accounts, savedAuth, activeAccountUuid, onClose, onSwitchAccount, onAddAccount, onLogoutRequest, authPending = false }: {
    accounts: StoredAuth[];
    savedAuth: StoredAuth | null;
    activeAccountUuid: string | null;
    onClose: () => void;
    onSwitchAccount: (account: StoredAuth) => void;
    onAddAccount: () => void;
    onLogoutRequest: (account: StoredAuth) => void;
    authPending?: boolean;
  } = $props();
  const { t } = createTranslation();
  let accountListOpen = $state(false);
  const isActiveAccount = (account: StoredAuth) => account.uuid != null
    ? account.uuid === activeAccountUuid : account.username === savedAuth?.username;
  const activeAccount = $derived(accounts.find(isActiveAccount) ?? savedAuth ?? accounts[0] ?? null);
  const canSwitchAccounts = $derived(accounts.length > 1);
  const orderedAccounts = $derived(activeAccount
    ? [...accounts.filter((account) => !isActiveAccount(account)), activeAccount] : []);
  const visibleAccounts = $derived(accountListOpen ? orderedAccounts : activeAccount ? [activeAccount] : []);
  const accountBackground = (active: boolean) => active
    ? accountListOpen ? "rgba(61,168,92,.14)" : "rgba(255,255,255,.06)" : "transparent";
</script>

<div class="absolute inset-0 z-48" onclick={onClose} role="presentation"></div>
<div class="absolute bottom-10 right-2 z-50 w-54 overflow-hidden" style={css({
  background: "rgba(31,30,28,.62)", backdropFilter: "blur(26px) saturate(150%)",
  WebkitBackdropFilter: "blur(26px) saturate(150%)", borderRadius: 9,
  boxShadow: "0 12px 28px rgba(0,0,0,.28)",
})}>
  {#if activeAccount}
    <div class="p-1.5">
      <div class="sb overflow-hidden" style={css({ display: "flex", flexDirection: "column",
        gap: accountListOpen ? 2 : 0, transition: "gap .22s cubic-bezier(.2,0,0,1)" })}>
        {#each visibleAccounts as account, index (account.uuid ?? account.username)}
          {@const isActive = isActiveAccount(account)}
          {@const offset = accountListOpen && !isActive ? Math.max(0, orderedAccounts.length - index - 1) : 0}
          <div style={css({ maxHeight: 40, opacity: 1,
            transform: `translateY(${offset * 4}px) scale(1)`, transformOrigin: "bottom center",
            pointerEvents: "auto", overflow: "hidden",
            animation: accountListOpen && !isActive ? "accountOptionIn .24s cubic-bezier(.2,0,0,1) both" : undefined,
            animationDelay: accountListOpen && !isActive ? `${offset * 18}ms` : undefined,
            transition: "opacity .2s cubic-bezier(.2,0,0,1), transform .28s cubic-bezier(.2,0,0,1)",
          })}>
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="group flex items-center gap-1 rounded-[7px]" style={css({
              background: accountBackground(isActive),
              transition: "background .18s cubic-bezier(.2,0,0,1), color .18s cubic-bezier(.2,0,0,1)",
            })} onmouseenter={(event) => {
              if (!isActive || !accountListOpen) event.currentTarget.style.background = "rgba(255,255,255,.08)";
            }} onmouseleave={(event) => { event.currentTarget.style.background = accountBackground(isActive); }}>
              <button onclick={() => {
                if (isActive) { if (canSwitchAccounts) accountListOpen = !accountListOpen; }
                else onSwitchAccount(account);
              }} class="min-w-0 flex-1 flex items-center gap-2 px-2 py-2 text-left"
                style={css({ background: "transparent", border: "none", cursor: isActive && !canSwitchAccounts ? "default" : "pointer" })}>
                <div class="w-6 h-6 rounded-md shrink-0 flex items-center justify-center text-[11px] font-medium text-t1 overflow-hidden bg-transparent">
                  <AccountAvatar skinId={account.uuid ?? account.username} size={24} fallback={(account.username ?? "?")[0].toUpperCase()} />
                </div>
                <div class="flex-1 min-w-0">
                  <p class="text-xs overflow-hidden text-ellipsis whitespace-nowrap" style={css({
                    color: isActive && accountListOpen ? C.green : C.t1, fontWeight: isActive ? 560 : 500,
                  })}>{account.username ?? "Unknown"}</p>
                </div>
              </button>
              {#if accountListOpen || !canSwitchAccounts}
                <button onclick={(event) => { event.stopPropagation(); onLogoutRequest(account); }}
                  title={t("auth.logout")} class="mr-1 flex h-7 w-7 items-center justify-center rounded-md text-t3"
                  style="background:transparent;border:none;cursor:pointer;transition:background .16s cubic-bezier(.2,0,0,1),color .16s cubic-bezier(.2,0,0,1)"
                  onmouseenter={(event) => { event.currentTarget.style.background = C.dangerBg; event.currentTarget.style.color = C.danger; }}
                  onmouseleave={(event) => { event.currentTarget.style.background = "transparent"; event.currentTarget.style.color = C.t3; }}>
                  <LogOut size={11} />
                </button>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    </div>
  {/if}
  {#if !savedAuth && accounts.length === 0}
    <div class="p-3">
      <button disabled={authPending} aria-busy={authPending} class="btn-phys w-full py-2.75 rounded-md cursor-pointer disabled:cursor-wait text-[13px] font-[inherit]" onclick={onAddAccount}
        style={css({ background: "rgba(255,255,255,.07)", border: `1px solid ${C.b1}`, color: C.t1 })}>
        {t(authPending ? "auth.logging_in" : "auth.login_microsoft")}
      </button>
    </div>
  {/if}
  <div class="px-1.5 pb-1.5 pt-0.5" style="border-top:none">
    <button disabled={authPending} aria-busy={authPending} class="btn-ghost btn-phys w-full flex items-center gap-1.75 px-2.5 py-2 rounded-md text-t2 text-xs cursor-pointer disabled:cursor-wait transition-all duration-100 text-left" onclick={onAddAccount}
      onmouseenter={(event) => { event.currentTarget.style.background = C.hover; event.currentTarget.style.color = C.t1; }}
      onmouseleave={(event) => { event.currentTarget.style.background = "transparent"; event.currentTarget.style.color = C.t2; }}>
      <UserPlus size={11} /> {t(authPending ? "auth.logging_in" : "auth.add_account")}
    </button>
  </div>
</div>
