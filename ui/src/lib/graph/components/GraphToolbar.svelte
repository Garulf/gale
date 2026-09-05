<script>
  import { useSvelteFlow } from '@xyflow/svelte';
  import { PALETTE, paletteEntryId } from '../palette.js';

  let {
    showEdgeLabels,
    canUndo,
    canRedo,
    saving,
    dirty,
    onAutoLayout,
    onUndo,
    onRedo,
    onSave,
    onDiscard,
    onToggleLabels,
    onAddNode,
  } = $props();

  let addMenuOpen = $state(false);

  const { fitView, screenToFlowPosition } = useSvelteFlow();

  function centerPosition() {
    return screenToFlowPosition({ x: window.innerWidth / 2, y: window.innerHeight / 2 });
  }

  function add(entry) {
    onAddNode(entry, centerPosition());
    addMenuOpen = false;
  }
</script>

<div class="gale-toolbar">
  <div>
    <div class="add-node-menu">
      <button type="button" onclick={() => (addMenuOpen = !addMenuOpen)}>+ Node</button>
      {#if addMenuOpen}
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
    <div class="group">
      <button type="button" onclick={onAutoLayout}>Auto layout</button>
      <button type="button" onclick={() => fitView()}>Fit view</button>
      <button type="button" disabled={!canUndo} onclick={onUndo}>Undo</button>
      <button type="button" disabled={!canRedo} onclick={onRedo}>Redo</button>
      <button type="button" onclick={onToggleLabels}>Labels {showEdgeLabels ? 'on' : 'off'}</button>
    </div>
  </div>
  <div>
    {#if dirty}
      <span class="unsaved">unsaved changes</span>
      <button type="button" class="discard" disabled={saving} onclick={onDiscard}>Discard</button>
    {/if}
    <button type="button" class="save" class:dirty data-testid="graph-save" disabled={saving} onclick={onSave}>Save profile</button>
  </div>
</div>
