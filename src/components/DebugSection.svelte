<script lang="ts">
  import type { Snippet } from "svelte";
  let { title, children }: { title: string; children: Snippet } = $props();
</script>

<div class="rounded-[6px] px-[14px] py-3 bg-[rgba(0,0,0,.22)]">
  <div class="text-[10px] font-semibold tracking-[0.10em] text-green uppercase mb-[10px] opacity-70">{title}</div>
  <div class="flex flex-col gap-[6px]">{@render children()}</div>
</div>
