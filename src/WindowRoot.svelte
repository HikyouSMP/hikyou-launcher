<script lang="ts">
  import type { Component } from "svelte";
  import ErrorBoundary from "./components/ErrorBoundary.svelte";
  let { component: Application }: { component: Component } = $props();
</script>

<ErrorBoundary><Application /></ErrorBoundary>
