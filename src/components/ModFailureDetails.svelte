<script lang="ts">
  import { createTranslation } from "../lib/i18n.svelte";
  import type { ModLifecycleFailure } from "../modLifecycleFailure";
  let { failures }: { failures: ModLifecycleFailure[] } = $props();
  const { t } = createTranslation();
</script>

{#if failures.length}
  <details data-selectable class="px-3 pb-2 text-[11px] text-t2">
    <summary class="cursor-pointer">{t("mod_failures.details")}</summary>
    {#each failures as failure}
      <pre data-mod-failure={failure.code} class="m-0 mt-1 whitespace-pre-wrap break-words select-text font-mono text-[10px]">{failure.operation} / {failure.code} / {failure.kind}{"\n"}{t(`mod_failures.${failure.code}`)}</pre>
    {/each}
  </details>
{/if}
