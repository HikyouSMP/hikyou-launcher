<script lang="ts">
  import { onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { createTranslation } from "../lib/i18n.svelte";
  import { C } from "../theme";
  let { value, isMac, onConfirm }: { value: string; isMac: boolean; onConfirm: (shortcut: string) => void } = $props();
  const { t } = createTranslation();
  let recording = $state(false);
  let confirmed = false;
  const parts = $derived(value.split("+"));

  function codeToDisplay(code: string): string | null {
    if (["Space", "Enter", "Tab", "Backspace"].includes(code)) return code;
    if (code.startsWith("Key") && code.length === 4) return code.slice(3);
    if (code.startsWith("Digit") && code.length === 6) return code.slice(5);
    if (/^F\d{1,2}$/.test(code)) return code;
    return null;
  }
  function startRecording() {
    confirmed = false;
    invoke("suspend_shortcut").catch(console.error);
    recording = true;
  }
  function stopRecording() {
    if (!recording) return;
    if (!confirmed) invoke("register_shortcut", { shortcutStr: value }).catch(console.error);
    confirmed = false;
    recording = false;
  }
  function handleKeyDown(event: KeyboardEvent) {
    if (!recording) {
      if (event.key === "Enter" || event.key === " ") { event.preventDefault(); startRecording(); }
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    if (["Control", "Alt", "Shift", "Meta"].includes(event.key)) return;
    const mods: string[] = [];
    if (event.ctrlKey) mods.push("Ctrl");
    if (event.altKey) mods.push(isMac ? "Option" : "Alt");
    if (event.shiftKey) mods.push("Shift");
    if (event.metaKey) mods.push("Cmd");
    if (!mods.length) return;
    const key = codeToDisplay(event.code);
    if (!key) return;
    confirmed = true;
    onConfirm([...mods, key].join("+"));
    recording = false;
  }
  onDestroy(stopRecording);
</script>

<div role="button" tabindex="0" onclick={startRecording} onblur={stopRecording} onkeydown={handleKeyDown}
  class="flex items-center gap-1 px-2 py-1.25 rounded-md cursor-pointer outline-none"
  style:border={`1px solid ${recording ? C.greenBdr : C.b1}`}
  style:background={recording ? C.greenBg : "transparent"} style:min-width="100px">
  {#if recording}
    <span class="text-[11px] text-t3">{t("settings.shortcut_recording")}</span>
  {:else}
    {#each parts as part}
      <span class="text-[10px] px-1.5 py-0.5 rounded-[3px] text-t2 font-mono" style:background={C.hover}>{part}</span>
    {/each}
  {/if}
</div>
