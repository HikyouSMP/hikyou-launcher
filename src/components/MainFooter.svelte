<script lang="ts">
  import { createTranslation } from "../lib/i18n.svelte";
  import { css } from "../lib/dom";
  import { C } from "../theme";
  import type { StoredAuth } from "../types";
  import type { SearchSuggestion as Suggestion, SearchSuggestionAction } from "../search/suggestions";
  import SearchSuggestion from "./SearchSuggestion.svelte";
  let { appVersion, savedAuth, accountsOpen, onVersionClick, onToggleAccounts, onLogin, suggestion, onSuggestionAction, unlockRemaining, authPending = false }: {
    appVersion: string; savedAuth: StoredAuth | null; accountsOpen: boolean;
    onVersionClick: () => void; onToggleAccounts: () => void; onLogin: () => void;
    suggestion: Suggestion | null; onSuggestionAction: (action: SearchSuggestionAction) => void;
    unlockRemaining: number | null;
    authPending?: boolean;
  } = $props();
  const { t } = createTranslation();
</script>

<div data-command-footer style={css({ borderTop: `1px solid ${C.b2}`, display: "grid", gridTemplateColumns: "minmax(0, 1fr) auto minmax(0, 1fr)", alignItems: "center", padding: "0 12px", flexShrink: 0, height: 36, gap: 8 })}>
  <button type="button" data-advanced-trigger style={css({ justifySelf: "start", fontSize: 11, color: C.t3, letterSpacing: "0", cursor: "default", minWidth: 0, maxWidth: "100%", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", background: "transparent", border: 0, padding: 0, fontFamily: "inherit" })} onclick={onVersionClick}>Hikyou{appVersion && ` v${appVersion}`}</button>
  <div style="min-width:0;justify-self:center">
    {#if unlockRemaining !== null}
      <span role="status" class="unlock-progress" data-advanced-progress>{t("advanced_confirm.remaining", { count: unlockRemaining })}</span>
    {:else}<SearchSuggestion {suggestion} onAction={onSuggestionAction} />{/if}
  </div>
  {#if savedAuth}
    <button onclick={onToggleAccounts} data-account-trigger="true" data-open={accountsOpen ? "true" : "false"}
      class="justify-self-end"
      style={css({ display: "flex", alignItems: "center", height: 24, background: accountsOpen ? C.hover : "transparent", border: "none", borderRadius: C.r, padding: "0 8px", color: C.t2, fontSize: 11, cursor: "pointer", letterSpacing: "-.01em", transition: "background .16s cubic-bezier(.2,0,0,1)", fontFamily: "inherit" })}
      onmouseenter={(event) => { if (!accountsOpen) event.currentTarget.style.background = C.hover; }}
      onmouseleave={(event) => { if (!accountsOpen) event.currentTarget.style.background = "transparent"; }}>
      <span style={css({ maxWidth: 118, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontWeight: 560 })}>{savedAuth.username ?? "Player"}</span>
    </button>
  {:else}
    <button onclick={onLogin} disabled={authPending} aria-busy={authPending} class="justify-self-end" style={css({ display: "flex", alignItems: "center", height: 24, background: "transparent", border: "1px solid transparent", borderRadius: C.r, padding: "0 8px", color: C.t3, fontSize: 11, cursor: authPending ? "wait" : "pointer", letterSpacing: "0", transition: "color .12s, border-color .12s, background .12s", fontFamily: "inherit" })}
      onmouseenter={(event) => { event.currentTarget.style.color = C.t2; event.currentTarget.style.borderColor = C.b1; event.currentTarget.style.background = C.hover; }}
      onmouseleave={(event) => { event.currentTarget.style.color = C.t3; event.currentTarget.style.borderColor = "transparent"; event.currentTarget.style.background = "transparent"; }}>{t(authPending ? "auth.logging_in" : "auth.login_button")}</button>
  {/if}
</div>

<style>
  .unlock-progress { display: block; max-width: 260px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11px; color: var(--color-t2); }
</style>
