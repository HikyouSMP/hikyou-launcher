<script lang="ts">
  import type { Snippet } from "svelte";
  let { children, onClick, title, quiet = false }: { children: Snippet; onClick: () => void; title?: string; quiet?: boolean } = $props();
</script>

<button {title} onclick={onClick}
  class="icon-btn btn-phys shrink-0" class:quiet style:width="32px" style:height="32px"
>{@render children()}</button>
