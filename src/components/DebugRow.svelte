<script lang="ts">
  import { onDestroy } from "svelte";
  import { Copy, FolderOpen } from "@lucide/svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { C } from "../theme";
  import { css } from "../lib/dom";
  let { k, v, mono, highlight, warn, copyable, openable }: { k: string; v: string; mono?: boolean; highlight?: boolean; warn?: boolean; copyable?: boolean; openable?: boolean } = $props();
  let copied = $state(false);
  let color = $derived(highlight ? C.green : warn ? C.warning : C.t1);
  let timer: ReturnType<typeof setTimeout> | undefined;
  function copy() {
    void navigator.clipboard.writeText(v);
    copied = true;
    clearTimeout(timer);
    timer = setTimeout(() => { copied = false; }, 1500);
  }
  onDestroy(() => clearTimeout(timer));
</script>

<div class="debug-row flex items-start gap-2 min-h-5">
  <span class="text-[10px] text-t3 w-[140px] shrink-0 pt-[1px] tracking-[0.01em] leading-[1.5]">{k}</span>
  <span class="text-[10px] flex-1 break-all leading-[1.5]" style={css({ color, fontFamily: mono ? "'JetBrains Mono','SF Mono','Fira Code',monospace" : "inherit" })}>{v || "—"}</span>
  {#if copyable && v}
    <button onclick={copy} class={`debug-row-action flex items-center rounded-[3px] cursor-pointer text-[9px] px-[5px] py-[3px] shrink-0 transition-[opacity,color] duration-[120ms] ${copied ? "copied" : ""}`}
      style={css({ background: "none", border: `1px solid ${C.b1}`, color: copied ? C.green : C.t3 })}>
      <span class="inline-flex w-[9px] h-[9px] items-center justify-center text-[9px] leading-none">{#if copied}✓{:else}<Copy size={9} />{/if}</span>
    </button>
  {/if}
  {#if openable && v}
    <button onclick={() => invoke("open_folder", { path: v }).catch(console.error)} class="debug-row-action flex items-center rounded-[3px] cursor-pointer text-[9px] px-[5px] py-[3px] shrink-0 transition-[opacity,color] duration-[120ms]"
      style={css({ background: "none", border: `1px solid ${C.b1}`, color: C.t3 })}
      onmouseenter={(e) => { e.currentTarget.style.color = C.t2; }} onmouseleave={(e) => { e.currentTarget.style.color = C.t3; }}><FolderOpen size={9} /></button>
  {/if}
</div>
