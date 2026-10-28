<script lang="ts">
  import type { Snippet } from "svelte";
  import { C } from "../theme";
  import { css } from "../lib/dom";
  let { children, onClick, disabled, tone, icon }: { children: Snippet; onClick?: () => void; disabled?: boolean; tone?: "amber" | "blue"; icon?: Snippet } = $props();
  let bg = $derived(tone === "amber" ? "rgba(232,162,58,.1)" : tone === "blue" ? "rgba(90,158,255,.1)" : "rgba(255,255,255,.06)");
  let col = $derived(tone === "amber" ? C.warning : tone === "blue" ? "#5a9eff" : C.t2);
</script>

<button onclick={onClick} {disabled} class="btn-phys flex items-center gap-1 px-[10px] py-[5px] rounded-[6px] cursor-pointer text-[10px] font-normal transition-opacity duration-[120ms]"
  style={css({ background: bg, border: "none", color: col, opacity: disabled ? 0.5 : 1 })}>
  {@render icon?.()}{@render children()}
</button>
