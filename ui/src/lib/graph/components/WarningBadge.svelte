<script>
  let { messages } = $props();
</script>

{#if messages.length > 0}
  <span class="warn-badge" data-node-warning title={messages.join('\n')} aria-label={messages.join('. ')}>!</span>
{/if}
