<script lang="ts">
  import type { Snippet } from "svelte";
  import { getCached, fetchAvatar } from "../avatarCache";

  let { skinId, size, fallback }: {
    skinId: string | null | undefined;
    size: number;
    fallback: Snippet | string;
  } = $props();
  let url = $state<string | null>(null);

  $effect(() => {
    const id = skinId;
    let active = true;
    url = id ? getCached(id) ?? null : null;
    if (id) {
      void fetchAvatar(id).then((next) => {
        if (active) url = next;
      }).catch(() => {});
    }
    return () => { active = false; };
  });
</script>

{#if url}
  <img src={url} width={size} height={size} class="block" style="image-rendering:pixelated" alt="" />
{:else if typeof fallback === "string"}
  {fallback}
{:else}
  {@render fallback()}
{/if}
