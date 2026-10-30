<script lang="ts">
  import { createTranslation } from "../lib/i18n.svelte";
  const { t } = createTranslation();
  import { C } from "../theme";
  import { assignRef, css } from "../lib/dom";
  import type { Profile } from "../types";
  import type { OptionsCopyPulse } from "./ProfileRows";
  import { loaderBadgeClass, loaderDisplayLabel } from "../utils/profileDisplay";
  let { profile, focused, hovered, ctrlHeld, ctrlIndex, optionsCopySource, optionsCopyPulse, smartStatus, navRef, onHoverChange, onActivate, onDelete, onContextMenu, onMiddlePick, onMiddleDrop }: {
    profile: Profile; focused: boolean; hovered: boolean; ctrlHeld: boolean; ctrlIndex: number; optionsCopySource: boolean; optionsCopyPulse: OptionsCopyPulse; smartStatus?: string | null;
    navRef: (element: HTMLElement | null) => void; onHoverChange: (hovered: boolean) => void; onActivate: () => void; onDelete: () => void; onContextMenu: (event: MouseEvent & { currentTarget: EventTarget & HTMLElement }) => void;
    onMiddlePick: (profile: Profile, event: MouseEvent & { currentTarget: EventTarget & HTMLElement }) => void;
    onMiddleDrop: (profile: Profile, event: MouseEvent & { currentTarget: EventTarget & HTMLElement }) => void;
  } = $props();
  let smart = $derived(profile.kind === "smart");
</script>

<div style="display:contents">
  <div role="button" tabindex="0" use:assignRef={navRef} onclick={onActivate} oncontextmenu={onContextMenu}
    onmousedown={(event) => { if (event.button === 1) { event.preventDefault(); onMiddlePick(profile, event); } }}
    onmouseup={(event) => { if (event.button === 1) { event.preventDefault(); onMiddleDrop(profile, event); } }}
    onkeydown={(event) => { if (event.key === "Enter") onActivate(); }} class="pi"
    data-command-selected={focused}
    data-options-copy={optionsCopySource ? "source" : undefined} data-options-copy-pulse={optionsCopyPulse?.kind ?? undefined}
    style={css({ "--ripple-x": `${optionsCopyPulse?.x ?? 50}%`, "--ripple-y": `${optionsCopyPulse?.y ?? 50}%`, display: "flex", alignItems: "center", gap: 8, padding: "7px 12px", margin: "0 6px 1px", borderRadius: C.r, cursor: "pointer", background: focused ? "rgba(255,255,255,.085)" : optionsCopySource ? "rgba(255,255,255,.075)" : hovered ? "rgba(255,255,255,.065)" : "transparent", transition: "none", outline: "none", position: "relative" })}
    onmousemove={(event) => { if (event.movementX || event.movementY) onHoverChange(true); }} onmouseleave={() => onHoverChange(false)}>
    <div style="flex:1;min-width:0"><div style={css({ display: "flex", alignItems: "center", gap: 6, flexWrap: "wrap" })}>
      <p style={css({ fontSize: 13, fontWeight: 400, color: C.t1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", flexShrink: 1 })}>{profile.name}</p>
      <span class={`chip ${loaderBadgeClass(profile.loader)}`} style={css({ fontSize: 10, fontWeight: 500, padding: "1px 5px", whiteSpace: "nowrap", flexShrink: 0, letterSpacing: "0.02em" })}>{loaderDisplayLabel(profile.loader)}</span>
      <span class="chip" style={css({ fontSize: 10, fontWeight: 400, padding: "1px 6px", background: "rgba(255,255,255,.055)", color: C.t2, whiteSpace: "nowrap", flexShrink: 0, fontFamily: "'JetBrains Mono','SF Mono',monospace" })}>{profile.mcVersion}</span>
      {#if smartStatus}<span class="smart-status-chip">{smartStatus}</span>{/if}
    </div></div>
    <div style={css({ display: "flex", alignItems: "center", gap: 3, flexShrink: 0 })}>
      {#if !smart}
        <button class="del" title={t("profile.retire_action")} aria-label={t("profile.retire_action")} onclick={(event) => { event.stopPropagation(); onDelete(); }}
          style={css({ width: 20, height: 20, display: "flex", alignItems: "center", justifyContent: "center", borderRadius: 5, background: "transparent", border: "none", color: C.t3, fontSize: 14, cursor: "pointer", lineHeight: 1, transition: "all .12s" })}
          onmouseenter={(event) => { event.currentTarget.style.color = C.danger; event.currentTarget.style.background = C.dangerBg; }}
          onmouseleave={(event) => { event.currentTarget.style.color = C.t3; event.currentTarget.style.background = "transparent"; }}>×</button>
      {/if}
      {#if ctrlHeld && ctrlIndex >= 0 && ctrlIndex < 10}
        <span style={css({ position: "absolute", right: 10, top: "50%", width: 20, height: 20, borderRadius: 5, background: focused ? "rgba(255,255,255,.075)" : "rgba(255,255,255,.06)", color: focused ? C.t2 : C.t3, fontSize: 11, fontFamily: "'JetBrains Mono','SF Mono',monospace", fontWeight: 400, display: "flex", alignItems: "center", justifyContent: "center", zIndex: 2, pointerEvents: "none", animation: "ctrlBadgeIn .15s cubic-bezier(.16,1,.3,1) forwards" })}>{ctrlIndex === 9 ? "0" : String(ctrlIndex + 1)}</span>
      {/if}
    </div>
  </div>
</div>
