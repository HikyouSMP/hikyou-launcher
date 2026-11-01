<script lang="ts">
  import type { Snippet } from "svelte";
  let { children, onClick, disabled, tone, icon }: { children: Snippet; onClick?: () => void; disabled?: boolean; tone?: "amber" | "blue"; icon?: Snippet } = $props();
</script>

<button onclick={onClick} {disabled} data-tone={tone} class="action-button btn-phys flex items-center gap-1 px-[10px] py-[5px] text-[10px] font-normal">
  {@render icon?.()}{@render children()}
</button>
