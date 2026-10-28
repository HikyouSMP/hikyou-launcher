<script lang="ts">
  import { createTranslation } from "../lib/i18n.svelte";
  const { t } = createTranslation();
  let { code }: { code: string } = $props();
</script>

<div class="flex items-center justify-between rounded-[7px] px-[11px] py-[7px] mb-[10px] cursor-pointer bg-[rgba(0,0,0,.4)]"
  role="button" tabindex="0" onclick={() => navigator.clipboard.writeText(code)}
  onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); void navigator.clipboard.writeText(code); } }}>
  <code class="text-green text-[10px]" style="font-family:'JetBrains Mono','SF Mono','Fira Code',monospace">{code}</code>
  <span class="text-t3 text-[9px] ml-2 shrink-0">{t("ui.copy_snippet")}</span>
</div>
