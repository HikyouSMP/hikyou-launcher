<script lang="ts">
  import { css } from "../lib/dom";
  import { C } from "../theme";
  import { CHIP_REVEAL_MOTION } from "../utils/chipMotion";
  let { label, animated = false }: { label: string; animated?: boolean } = $props();
</script>

<span class="chip search-mode-chip" class:animated
  style={css({ "--mode-chip-bg": C.hover, "--mode-chip-duration": `${CHIP_REVEAL_MOTION.durationMs}ms`, "--mode-chip-spring": CHIP_REVEAL_MOTION.easing, color: C.t2 })}>
  <span class="search-mode-chip-label">{label}</span>
</span>

<style>
  .search-mode-chip { position: relative; display: inline-flex; align-items: center; flex-shrink: 0; min-height: 18px; padding: 2px 7px; overflow: hidden; border-radius: 9999px; font-size: 10px; line-height: 14px; letter-spacing: 0; white-space: nowrap; }
  .search-mode-chip::before { content: ""; position: absolute; inset: 0; border-radius: inherit; background: var(--mode-chip-bg); transform-origin: right center; }
  .search-mode-chip-label { position: relative; }
  .animated::before { animation: chip-background-in var(--mode-chip-duration) cubic-bezier(.2, 0, .2, 1) both; }
  .animated .search-mode-chip-label { animation: chip-label-in var(--mode-chip-duration) cubic-bezier(.2, 0, .2, 1) both; }
  @supports (animation-timing-function: linear(0, 1)) {
    .animated::before, .animated .search-mode-chip-label { animation-timing-function: var(--mode-chip-spring); }
  }
  @keyframes chip-background-in { from { transform: scaleX(0); } to { transform: scaleX(1); } }
  @keyframes chip-label-in { from { transform: translateX(calc(100% + 7px)); } to { transform: translateX(0); } }
  @media (prefers-reduced-motion: reduce) {
    .animated::before, .animated .search-mode-chip-label { animation: none; }
  }
</style>
