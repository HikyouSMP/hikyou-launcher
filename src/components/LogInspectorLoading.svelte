<script lang="ts">
</script>

<div class="tool-window log-inspector-window log-loading-shell">
  <aside class="log-loading-sidebar">
    <div class="log-loading-label"></div><div class="log-loading-select"></div>
    <div class="log-loading-label short"></div><div class="log-loading-select"></div>
    <div class="log-loading-card"><span></span><span></span><span></span></div>
  </aside>
  <main class="log-loading-main">
    <div class="log-loading-head">
      <div class="log-loading-search"></div><div class="log-loading-pill"></div><div class="log-loading-pill sm"></div>
    </div>
    <div class="log-loading-lines">
      {#each Array.from({ length: 13 }) as _, i}
        <div class="log-loading-line"><span></span><b style:width={`${42 + ((i * 17) % 48)}%`}></b></div>
      {/each}
    </div>
  </main>
</div>
