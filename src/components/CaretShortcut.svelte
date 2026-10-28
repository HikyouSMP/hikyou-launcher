<script lang="ts">
  import { tick, onDestroy } from "svelte";
  import type { SearchSuggestion as Suggestion, SearchSuggestionAction } from "../search/suggestions";
  import SearchSuggestion from "./SearchSuggestion.svelte";

  let { input, value, suggestion, onAction }: {
    input: HTMLInputElement | undefined; value: string; suggestion: Suggestion | null;
    onAction: (action: SearchSuggestionAction) => void;
  } = $props();
  let mirror = $state<HTMLSpanElement>();
  let hint = $state<HTMLDivElement>();
  let left = $state<number | null>(null);
  let composing = false;
  let active = true;
  onDestroy(() => { active = false; });
  const schedule = () => queueMicrotask(position);

  function position() {
    if (!active) return;
    left = null;
    if (!input || !mirror || !hint || !suggestion || composing || document.activeElement !== input
      || input.selectionStart !== value.length || input.selectionEnd !== value.length || !mirror.firstChild
      || input.value !== value || mirror.textContent !== value) return;
    const style = getComputedStyle(input);
    mirror.style.font = style.font;
    mirror.style.letterSpacing = style.letterSpacing;
    mirror.style.direction = style.direction;
    // Browser text layout handles proportional fonts, Unicode and bidirectional runs.
    const range = document.createRange();
    range.setStart(mirror.firstChild, value.length);
    range.collapse(true);
    const caret = range.getBoundingClientRect();
    const x = caret.left - mirror.getBoundingClientRect().left + parseFloat(style.paddingLeft || "0") - input.scrollLeft + 8;
    if (x >= 0 && x + hint.offsetWidth <= input.clientWidth) left = x;
  }

  $effect(() => { value; suggestion; mirror; hint; void tick().then(position); });
  $effect(() => {
    const node = input;
    if (!node) return;
    const startComposition = () => { composing = true; schedule(); };
    const endComposition = () => { composing = false; void tick().then(position); };
    const events = ["keyup", "click", "select", "scroll", "focus", "blur"] as const;
    for (const event of events) node.addEventListener(event, schedule);
    node.addEventListener("compositionstart", startComposition);
    node.addEventListener("compositionend", endComposition);
    document.addEventListener("selectionchange", schedule);
    document.fonts.addEventListener("loadingdone", schedule);
    const observer = new ResizeObserver(schedule);
    observer.observe(node);
    return () => {
      observer.disconnect();
      for (const event of events) node.removeEventListener(event, schedule);
      node.removeEventListener("compositionstart", startComposition);
      node.removeEventListener("compositionend", endComposition);
      document.removeEventListener("selectionchange", schedule);
      document.fonts.removeEventListener("loadingdone", schedule);
    };
  });
</script>

{#if suggestion}
  <span bind:this={mirror} class="caret-mirror" aria-hidden="true">{value}</span>
  <div bind:this={hint} class="caret-shortcut" style:left={`${left ?? 0}px`} style:visibility={left === null ? "hidden" : "visible"}>
    <SearchSuggestion {suggestion} {onAction} compact />
  </div>
{/if}

<style>
  .caret-mirror { position: absolute; top: 0; left: 0; visibility: hidden; pointer-events: none; white-space: pre; }
  .caret-shortcut { position: absolute; top: 50%; transform: translateY(-50%); }
</style>
