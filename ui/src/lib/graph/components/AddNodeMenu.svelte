<script>
  import { useSvelteFlow } from '@xyflow/svelte';
  import { PALETTE, paletteEntryId } from '../palette.js';

  let { onAddNode } = $props();

  let open = $state(false);

  const { screenToFlowPosition } = useSvelteFlow();

  function centerPosition() {
    return screenToFlowPosition({ x: window.innerWidth / 2, y: window.innerHeight / 2 });
  }

  function add(entry) {
    onAddNode(entry, centerPosition());
    open = false;
  }
</script>

<div class="add-node-menu">
  <button type="button" aria-expanded={open} onclick={() => (open = !open)}>+ Node</button>
  {#if open}
    <div class="add-node-options">
      {#each PALETTE as group (group.label)}
        <span class="palette-group">{group.label}</span>
        {#each group.entries as entry (paletteEntryId(entry))}
          <button type="button" data-testid="add-node-{paletteEntryId(entry)}" onclick={() => add(entry)}><strong>{entry.label}</strong><small>{entry.hint}</small></button>
        {/each}
      {/each}
    </div>
  {/if}
</div>
