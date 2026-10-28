<script module lang="ts">
  export type { CreatingProfileDraft } from "./CreateCandidateList.svelte";
</script>

<script lang="ts">
  import { X } from "@lucide/svelte";
  import { C } from "../theme";
  import { assignRef, css } from "../lib/dom";
  import type { Ref } from "../lib/state";
  import type { CreatingProfileDraft } from "./CreateCandidateList.svelte";
  let { creating, inputRef, labelForLoader, namePlaceholder, onChangeName, onCancel, onSubmit }: {
    creating: CreatingProfileDraft; inputRef: Ref<HTMLInputElement | null>; labelForLoader: (loader: string) => string; namePlaceholder: string;
    onChangeName: (value: string) => void; onCancel: () => void; onSubmit: () => void;
  } = $props();
  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter") { e.preventDefault(); e.stopPropagation(); onSubmit(); return; }
    if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); onCancel(); }
  }
  function focusInput(node: HTMLInputElement) { node.focus(); }
</script>

<div class="glass-panel create-panel" style="margin:4px 8px;padding:10px 12px 12px">
  <div style={css({ display: "flex", alignItems: "center", gap: 8, marginBottom: 10 })}>
    <span style={css({ fontSize: 12, fontWeight: 500, color: C.t1 })}>{labelForLoader(creating.loader)} {creating.versionId}</span>
    <button class="icon-btn" onclick={onCancel} style="margin-left:auto"><X size={13} /></button>
  </div>
  <input class="minimal-input" use:assignRef={inputRef} use:focusInput type="text" value={creating.inputName} oninput={(e) => onChangeName(e.currentTarget.value)} placeholder={namePlaceholder} style="width:100%"
    onfocus={(e) => { e.currentTarget.style.borderColor = C.greenBdr; }} onblur={(e) => { e.currentTarget.style.borderColor = C.b1; }} onkeydown={handleKeyDown} />
</div>
