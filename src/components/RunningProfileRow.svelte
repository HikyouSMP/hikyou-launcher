<script lang="ts">
  import { C } from "../theme";
  import { css } from "../lib/dom";
  import type { Profile } from "../types";
  import { loaderBadgeClass, loaderDisplayLabel } from "../utils/profileDisplay";
  import ProfileProgressBar from "./ProfileProgressBar.svelte";
  import type { OptionsCopyPulse } from "./ProfileRows";
  let { profile, statusText, smartStatus, progressPercent, onContextMenu, optionsCopySource, optionsCopyPulse, onMiddlePick, onMiddleDrop }: {
    profile: Profile; statusText: string; smartStatus?: string | null; progressPercent?: number | null;
    onContextMenu: (event: MouseEvent & { currentTarget: EventTarget & HTMLElement }) => void; optionsCopySource?: boolean; optionsCopyPulse?: OptionsCopyPulse;
    onMiddlePick?: (profile: Profile, event: MouseEvent & { currentTarget: EventTarget & HTMLElement }) => void;
    onMiddleDrop?: (profile: Profile, event: MouseEvent & { currentTarget: EventTarget & HTMLElement }) => void;
  } = $props();
</script>

<div class="contents">
  <div role="button" tabindex="0" aria-label={`${profile.name}: ${statusText}`} class="pi running-item block mx-1.5 mb-px p-0 rounded-md overflow-hidden" data-options-copy={optionsCopySource ? "source" : undefined} data-options-copy-pulse={optionsCopyPulse?.kind ?? undefined}
    style={css({ "--ripple-x": `${optionsCopyPulse?.x ?? 50}%`, "--ripple-y": `${optionsCopyPulse?.y ?? 50}%`, background: optionsCopySource ? "rgba(255,255,255,.075)" : "rgba(61,168,92,.055)", backdropFilter: "blur(16px) saturate(145%)", WebkitBackdropFilter: "blur(16px) saturate(145%)" })}
    oncontextmenu={onContextMenu}
    onmousedown={(event) => { if (event.button === 1) { event.preventDefault(); onMiddlePick?.(profile, event); } }}
    onmouseup={(event) => { if (event.button === 1) { event.preventDefault(); onMiddleDrop?.(profile, event); } }}>
    <div class="relative flex items-center gap-2 px-3 py-1.75" style={css({ borderRadius: C.r, background: "rgba(61,168,92,.07)", paddingBottom: 7 })}>
      <div class="flex-1 min-w-0"><div class="flex items-center gap-1.25 flex-wrap">
        <p class="text-[13px] font-normal text-t1 overflow-hidden text-ellipsis whitespace-nowrap shrink">{profile.name}</p>
        <span class={loaderBadgeClass(profile.loader) + " chip text-[10px] font-medium px-1.25 py-px whitespace-nowrap shrink-0 tracking-[0.02em]"}>{loaderDisplayLabel(profile.loader)}</span>
        <span class="chip text-[9px] font-medium px-1.25 py-px text-t2 whitespace-nowrap shrink-0 font-mono" style="background:rgba(255,255,255,.06)">{profile.mcVersion}</span>
        {#if smartStatus}<span class="smart-status-chip">{smartStatus}</span>{/if}
      </div></div>
      <div class="flex items-center gap-1.5 shrink-0"><span class="text-green text-[11px] font-semibold">{statusText}</span></div>
      {#if progressPercent !== null && progressPercent !== undefined}<ProfileProgressBar percent={progressPercent} />{/if}
      {#if progressPercent === undefined}<ProfileProgressBar percent={null} />{/if}
    </div>
  </div>
</div>
