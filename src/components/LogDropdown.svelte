<script module lang="ts">
  export interface LogDropdownOption { value: string; label: string }
</script>

<script lang="ts">
  import { ChevronDown } from "@lucide/svelte";
  let { value, options, onChange, ariaLabel }: {
    value: string;
    options: LogDropdownOption[];
    onChange: (value: string) => void;
    ariaLabel?: string;
  } = $props();
  let open = $state(false);
  let root: HTMLDivElement;
  const current = $derived(options.find((option) => option.value === value));
  function outside(event: MouseEvent) {
    if (open && !root?.contains(event.target as Node)) open = false;
  }
</script>

<svelte:window onmousedown={outside} onkeydown={(event) => { if (event.key === "Escape") open = false; }} />
<div class={open ? "log-dropdown open" : "log-dropdown"} bind:this={root}>
  <button type="button" class="log-dropdown-trigger" aria-label={ariaLabel} aria-expanded={open} onclick={() => open = !open}>
    <span>{current?.label ?? ""}</span><ChevronDown size={14} />
  </button>
  {#if open}
    <div class="log-dropdown-menu">
      {#each options as option (option.value)}
        <button type="button" class={option.value === value ? "log-dropdown-item active" : "log-dropdown-item"}
          onclick={() => { onChange(option.value); open = false; }}>
          {option.label}
        </button>
      {/each}
    </div>
  {/if}
</div>
