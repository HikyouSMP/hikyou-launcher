<script lang="ts">
  import { RefreshCw } from "@lucide/svelte";
  import SBtn from "../../src/components/SBtn.svelte";
  import Pill from "../../src/components/Pill.svelte";
  import MainHeader from "../../src/components/MainHeader.svelte";
  const inputRef = { current: null as HTMLInputElement | null };
  let activeDebug = $state(false);
</script>

<div data-underlay style="background:#ffffff;width:750px;height:470px">
  <div class="app-main-shell" style="background:rgba(18,18,16,.74);padding:32px;box-sizing:border-box">
    <div style="display:flex;gap:16px;align-items:center">
      <SBtn>Refresh</SBtn><SBtn tone="amber">Check</SBtn><SBtn tone="blue">Browse</SBtn><SBtn disabled>Waiting</SBtn>
      <Pill title="Refresh" onClick={() => {}}><RefreshCw size={14} /></Pill>
      <button type="button" class="icon-btn" title="History"><RefreshCw size={14} /></button>
    </div>
    <div class="glass-panel" style="margin-top:32px;padding:18px;display:flex;gap:12px">
      <button type="button" class="modal-btn">Cancel</button>
      <button type="button" class="modal-btn primary">Apply</button>
      <button type="button" class="modal-btn danger">Remove</button>
      <button type="button" class="modal-btn primary" disabled>Pending</button>
      <button type="button" class="action-button px-3 py-1.5 text-[12px]">Disable</button>
    </div>
    <MainHeader {inputRef} searchValue="" searchMode="profile" versionChannel="release" debugVisible={true} {activeDebug} canOpenLogInspector={true}
      onSearchChange={() => {}} onOpenLogInspector={() => {}} onToggleDebug={() => { activeDebug = !activeDebug; }} onOpenSettings={() => {}} suggestion={null} onSuggestionAction={() => {}} />
  </div>
</div>
