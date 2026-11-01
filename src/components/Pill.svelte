<script lang="ts">
  import type { Snippet } from "svelte";
  import { C } from "../theme";
  let { children, onClick, title }: { children: Snippet; onClick: () => void; title?: string } = $props();
</script>

<button {title} onclick={onClick}
  class="btn-phys w-8 h-8 flex items-center justify-center rounded-[6px] shrink-0 cursor-pointer transition-[background,color] duration-100 bg-transparent border-0" style:color={C.controlText}
  onmouseenter={(e) => { e.currentTarget.style.background = C.hover; e.currentTarget.style.color = C.t1; }}
  onmouseleave={(e) => { e.currentTarget.style.background = "transparent"; e.currentTarget.style.color = C.controlText; }}
>{@render children()}</button>
