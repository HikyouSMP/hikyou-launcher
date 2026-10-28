<script lang="ts">
  import type { Snippet } from "svelte";
  let { children }: { children: Snippet } = $props();
</script>

<div class="px-4 py-10 text-center text-[12px] text-t3">{@render children()}</div>
