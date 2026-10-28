<script lang="ts">
  import { setContext, tick, type Snippet } from "svelte";
  import { assignRef, css } from "../lib/dom";
  import type { Ref } from "../lib/state";
  let { label, sub, children, last }: { label: string; sub?: string; children: Snippet; last?: boolean } = $props();
  setContext("hikyou-settings-label", () => label);
  let hov = $state(false);
  const subBoxRef: Ref<HTMLSpanElement | null> = { current: null };
  const subTextRef: Ref<HTMLSpanElement | null> = { current: null };
  let marqueeActive = $state(false);
  let marquee = $state({ overflowPx: 0, durationSec: 0 });

  $effect(() => {
    const text = sub;
    const hovered = hov;
    let frame: number | undefined;
    let cancelled = false;
    if (!text || !hovered) {
      marquee = { overflowPx: 0, durationSec: 0 };
      marqueeActive = false;
    } else {
      const measure = () => {
        if (cancelled || !subBoxRef.current || !subTextRef.current) return;
        const overflowPx = Math.ceil(subTextRef.current.scrollWidth - subBoxRef.current.clientWidth);
        marquee = { overflowPx: Math.max(0, overflowPx), durationSec: overflowPx > 0 ? Math.max(2.6, overflowPx / 42) : 0 };
      };
      void tick().then(() => { if (!cancelled) { measure(); frame = requestAnimationFrame(measure); } });
    }
    return () => { cancelled = true; if (frame !== undefined) cancelAnimationFrame(frame); };
  });
  $effect(() => {
    const overflow = marquee.overflowPx;
    marqueeActive = false;
    if (!hov || overflow <= 0) return;
    const timer = window.setTimeout(() => { marqueeActive = true; }, 1050);
    return () => window.clearTimeout(timer);
  });
</script>

<div role="group" aria-label={label} onmouseenter={() => { hov = true; }} onmouseleave={() => { hov = false; }}
  class="settings-row relative flex items-center justify-between px-4 py-[13px] overflow-visible"
  style={css({ borderBottom: last ? "none" : "1px solid rgba(255,255,255,.04)", minHeight: 50 })}>
  <div class="flex-1 min-w-0 pr-3">
    <div class="flex items-center gap-2 min-w-0">
      <p class="shrink-0 text-[13px] text-t1 font-normal leading-[1.3]">{label}</p>
      {#if sub}
        <span use:assignRef={subBoxRef} class="settings-sub min-w-0 overflow-hidden whitespace-nowrap text-[11px] leading-[1.3] pointer-events-none"
          style={css({ opacity: hov ? 1 : 0, transform: hov ? "translate3d(0, 1px, 0)" : "translate3d(-3px, 1px, 0)", color: "rgba(215,212,204,.78)", "--settings-sub-overflow": `${marquee.overflowPx}px`, "--settings-sub-duration": `${marquee.durationSec}s`, transition: "opacity .16s cubic-bezier(.2,0,0,1), transform .16s cubic-bezier(.2,0,0,1)" })}>
          <span use:assignRef={subTextRef} class={marqueeActive ? "settings-sub-track is-marquee" : "settings-sub-track"}>{sub}</span>
        </span>
      {/if}
    </div>
  </div>
  <div class="shrink-0 overflow-visible">{@render children()}</div>
</div>
