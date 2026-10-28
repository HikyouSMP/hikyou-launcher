<script lang="ts">
  import type { HTMLInputAttributes } from "svelte/elements";
  import { css } from "../lib/dom";
  type InputFocusEvent = FocusEvent & { currentTarget: EventTarget & HTMLInputElement };
  let { value, onCommit, min, max, step, placeholder, className, class: classAlias, style, onFocus, onBlur }: {
    value: number; onCommit: (value: number) => void; min?: number; max?: number; step?: number;
    placeholder?: string; className?: string; class?: string; style?: Record<string, string | number | null | undefined> | HTMLInputAttributes["style"];
    onFocus?: (event: InputFocusEvent) => void; onBlur?: (event: InputFocusEvent) => void;
  } = $props();
  let draft = $state("");
  $effect(() => { draft = String(value); });
  function commit() {
    const trimmed = draft.trim();
    let next = trimmed === "" ? 0 : Number(trimmed);
    if (!Number.isFinite(next)) next = 0;
    if (min != null) next = Math.max(min, next);
    if (max != null) next = Math.min(max, next);
    draft = String(next);
    onCommit(next);
  }
</script>

<input type="number" {min} {max} {step} value={draft} {placeholder} class={classAlias ?? className}
  style={typeof style === "object" && style !== null ? css(style) : style}
  oninput={(event) => { draft = event.currentTarget.value; }} onfocus={onFocus}
  onblur={(event) => { commit(); onBlur?.(event); }}
  onkeydown={(event) => { if (event.key === "Enter") event.currentTarget.blur(); }} />
