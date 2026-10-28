<script lang="ts">
  import type { Snippet } from "svelte";
  import { enterFromClass, type EnterFrom } from "../utils/viewTransitions";
  let { children, enterFrom }: { children: Snippet; enterFrom: EnterFrom } = $props();
  const animationClass = $derived(enterFromClass(enterFrom));
</script>

<div class={`${animationClass} flex flex-col flex-1 min-h-0 overflow-hidden`} data-view-route="mod-list">
  {@render children()}
</div>
