<script>
  let { active } = $props();
</script>

{#if active}
  <span class="override-badge" data-node-override title="manually overridden" aria-label="manually overridden">M</span>
{/if}
