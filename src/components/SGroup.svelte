<script lang="ts">
  import type { Snippet } from "svelte";
  let { label, children, actions }: { label: string; children: Snippet; actions?: Snippet } = $props();
</script>

<div>
  <div class="flex items-center justify-between gap-3 mb-2 pl-[2px]">
    <p class="text-[11px] font-medium tracking-[.06em] uppercase text-t2">{label}</p>
    {@render actions?.()}
  </div>
  <div class="rounded-[6px] bg-[rgba(255,255,255,.025)]">{@render children()}</div>
</div>
