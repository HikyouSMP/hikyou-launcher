<script lang="ts">
  import type { Snippet } from "svelte";
  import { createTranslation } from "../lib/i18n.svelte";
  let { children }: { children: Snippet } = $props();
  const { t } = createTranslation();
</script>

<svelte:boundary>
  {@render children()}
  {#snippet failed(error, reset)}
    <div class="h-screen flex items-center justify-center bg-app-bg p-6">
      <div class="rounded-lg p-5 max-w-95 bg-danger-bg border border-danger-bdr">
        <p class="text-danger text-[13px] font-semibold mb-1">
          {t("error_boundary.title")}
        </p>
        <p class="text-[rgba(252,165,165,.55)] text-[11px] break-all">{error instanceof Error ? error.message : String(error)}</p>
        <button type="button" onclick={reset} class="mt-3 px-3 py-1 rounded-md text-[11px] cursor-pointer text-[#a1a1aa] bg-[rgba(255,255,255,.07)] border-0">{t("common.retry")}</button>
      </div>
    </div>
  {/snippet}
</svelte:boundary>
