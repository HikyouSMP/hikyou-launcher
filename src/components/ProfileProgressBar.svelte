<script lang="ts">
  import { css } from "../lib/dom";
  let { percent }: { percent: number | null } = $props();
  let determinate = $derived(percent !== null);
</script>

<div aria-hidden="true" class={determinate ? "profile-progress determinate" : "profile-progress indeterminate"}>
  {#if percent !== null}<div class="profile-progress-fill" style={css({ width: `${percent}%`, minWidth: percent > 0 ? 8 : undefined })}></div>
  {:else}<span class="profile-progress-segment"></span>{/if}
</div>
