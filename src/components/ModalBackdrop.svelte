<script lang="ts">
  let { onClick, fixed = false, className = "" }: { onClick?: () => void; fixed?: boolean; className?: string } = $props();
</script>

<div class={`${fixed ? "fixed" : "absolute"} inset-0 modal-backdrop ${className}`} onclick={onClick} role="presentation"></div>
