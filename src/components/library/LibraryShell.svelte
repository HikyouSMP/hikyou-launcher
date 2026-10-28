<script lang="ts">
  import { ArrowLeft } from "@lucide/svelte";
  import type { Snippet } from "svelte";
  import { createTranslation } from "../../lib/i18n.svelte";
  let { title, subtitle, onBack, children }: { title: string; subtitle?: string; onBack: () => void; children: Snippet } = $props();
  const { t } = createTranslation();
</script>

<div class="flex h-full min-h-0 flex-1 flex-col overflow-hidden">
  <header data-tauri-drag-region class="flex h-15 shrink-0 items-center gap-3 px-4">
    <button class="icon-btn" onclick={onBack} title={t("common.back")}><ArrowLeft size={15} /></button>
    <div class="min-w-0 flex-1">
      <div class="truncate text-[16px] font-medium text-t1">{title}</div>
      {#if subtitle}<div class="truncate text-[11px] text-t3">{subtitle}</div>{/if}
    </div>
  </header>
  <div class="sb min-h-0 flex-1 overflow-y-auto">{@render children()}</div>
</div>
