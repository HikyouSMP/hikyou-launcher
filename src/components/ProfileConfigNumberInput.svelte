<script lang="ts">
  import NumberInput from "./NumberInput.svelte";
  let { value, onChange, min = 0, max, placeholder, unit, width = 72 }: {
    value: number; onChange: (value: number) => void; min?: number; max?: number; placeholder?: string; unit?: string; width?: number;
  } = $props();
</script>

<div class="flex items-center gap-1">
  <NumberInput {value} onCommit={onChange} {min} {max} {placeholder} className="pcp-num" style={{ width }} />
  {#if unit}<span class="text-[11px] text-t3 shrink-0">{unit}</span>{/if}
</div>
