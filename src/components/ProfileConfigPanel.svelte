<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { createTranslation } from "../lib/i18n.svelte";
  import { assignRef, css } from "../lib/dom";
  import type { Ref } from "../lib/state";
  import type { Profile } from "../types";
  import ModalBackdrop from "./ModalBackdrop.svelte";
  import NumInput from "./ProfileConfigNumberInput.svelte";
  interface GlobalDefaults { memoryMb: number; windowW: number; windowH: number }
  interface Props {
    profile: Profile; globalDefaults: GlobalDefaults; onClose: () => void;
    onSave: (id: string, changes: { name: string; memoryMb: number | null; windowW: number | null; windowH: number | null }) => void;
    onDelete: (id: string) => void; onOpenFolder?: () => void;
  }
  let { profile, globalDefaults, onClose, onSave, onDelete, onOpenFolder }: Props = $props();
  const { t } = createTranslation();
  let name = $state("");
  let useGlobalMem = $state(true);
  let memoryMb = $state(0);
  let useGlobalWin = $state(true);
  let windowW = $state(0);
  let windowH = $state(0);
  const panelRef: Ref<HTMLDivElement | null> = { current: null };

  // Editing follows profile identity; unrelated default changes must not erase a draft.
  $effect(() => {
    profile.id;
    untrack(() => {
      name = profile.name;
      useGlobalMem = profile.memoryMb == null;
      memoryMb = profile.memoryMb ?? globalDefaults.memoryMb;
      useGlobalWin = profile.windowW == null && profile.windowH == null;
      windowW = profile.windowW ?? globalDefaults.windowW;
      windowH = profile.windowH ?? globalDefaults.windowH;
    });
  });
  onMount(() => {
    const panel = panelRef.current;
    if (!panel) return;
    const trapFocus = (e: KeyboardEvent) => {
      if (e.key !== "Tab") return;
      const focusable = Array.from(panel.querySelectorAll<HTMLElement>("button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex='-1'])"));
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
      else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
    };
    panel.addEventListener("keydown", trapFocus);
    return () => panel.removeEventListener("keydown", trapFocus);
  });
  function focusInput(node: HTMLInputElement) { node.focus(); }
  function handleSave() { onSave(profile.id, { name, memoryMb: useGlobalMem ? null : memoryMb, windowW: useGlobalWin ? null : windowW, windowH: useGlobalWin ? null : windowH }); }
  function handleKeys(e: KeyboardEvent) {
    if (e.key === "Escape") { e.stopPropagation(); onClose(); }
    if ((e.ctrlKey || e.metaKey) && e.key === "s") { e.preventDefault(); handleSave(); onClose(); }
  }
</script>

<div class="fixed inset-0 z-[200] flex items-center justify-center">
  <ModalBackdrop onClick={onClose} />
  <div use:assignRef={panelRef} data-focus-scope="modal" onclick={(e) => e.stopPropagation()} onkeydown={handleKeys}
    role="dialog" aria-modal="true" aria-label={t("profile_config.name_label")} tabindex="-1" class="glass-panel modal-card pcp-modal"
    style={css({ width: "min(352px, calc(100vw - 32px))", maxHeight: "calc(100vh - 32px)", animation: "slideUp .18s cubic-bezier(.16,1,.3,1) forwards" })}>
    <input use:focusInput value={name} oninput={(e) => { name = e.currentTarget.value; }}
      onkeydown={(e) => { if (e.key === "Enter") { e.preventDefault(); handleSave(); onClose(); } if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); onClose(); } }}
      class="pcp-title-input" aria-label={t("profile_config.name_label")} />
    <div class="pcp-fields">
      <section class="pcp-field">
        <div class="pcp-field-head"><span>{t("profile_config.memory_label")}</span><strong>{useGlobalMem ? `${globalDefaults.memoryMb} MB` : `${memoryMb} MB`}</strong></div>
        <div class="pcp-field-controls">
          <button class={useGlobalMem ? "pcp-switch" : "pcp-switch active"} onclick={() => { useGlobalMem = !useGlobalMem; }}>{useGlobalMem ? "Global" : t("profile_config.custom_badge")}</button>
          {#if !useGlobalMem}<NumInput value={memoryMb} onChange={(value) => { memoryMb = value; }} min={512} max={65536} placeholder="2048" unit="MB" width={86} />{/if}
        </div>
      </section>
      <section class="pcp-field">
        <div class="pcp-field-head"><span>{t("profile_config.window_size_label")}</span><strong>{useGlobalWin ? `${globalDefaults.windowW}×${globalDefaults.windowH}` : `${windowW}×${windowH}`}</strong></div>
        <div class="pcp-field-controls">
          <button class={useGlobalWin ? "pcp-switch" : "pcp-switch active"} onclick={() => { useGlobalWin = !useGlobalWin; }}>{useGlobalWin ? "Global" : t("profile_config.custom_badge")}</button>
          {#if !useGlobalWin}<div class="pcp-inline-control">
            <NumInput value={windowW} onChange={(value) => { windowW = value; }} min={320} max={7680} placeholder="854" unit="W" width={62} />
            <span class="pcp-x">×</span>
            <NumInput value={windowH} onChange={(value) => { windowH = value; }} min={240} max={4320} placeholder="480" unit="H" width={62} />
          </div>{/if}
        </div>
      </section>
    </div>
    <div class="modal-actions pcp-actions">
      <button onclick={() => { onDelete(profile.id); onClose(); }} class="modal-btn danger pcp-delete">{t("common.delete")}</button>
      {#if onOpenFolder}<button onclick={onOpenFolder} class="modal-btn pcp-secondary">{t("common.folder")}</button>{/if}
      <button onclick={() => { handleSave(); onClose(); }} class="modal-btn primary pcp-save">{t("common.save")}</button>
    </div>
  </div>
</div>
