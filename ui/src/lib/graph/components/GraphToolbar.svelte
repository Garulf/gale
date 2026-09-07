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
    scopeName = '',
    scope = '',
    onExitScope,
    canGroup = false,
    onGroup,
    fitNodeIds = [],
  } = $props();

  let addMenuOpen = $state(false);

  const { fitView, screenToFlowPosition } = useSvelteFlow();

  function fitScope() {
    return fitNodeIds.length > 0 ? fitView({ nodes: fitNodeIds.map((id) => ({ id })) }) : fitView();
  }

  function centerPosition() {
    return screenToFlowPosition({ x: window.innerWidth / 2, y: window.innerHeight / 2 });
  }

  function add(entry) {
    onAddNode(entry, centerPosition());
    addMenuOpen = false;
  }

  let lastScope = '';

  $effect(() => {
    if (scope === lastScope) return;
    lastScope = scope;
    const timer = setTimeout(() => fitScope(), 0);
    return () => clearTimeout(timer);
  });
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
      <button type="button" onclick={() => fitScope()}>Fit view</button>
      <button type="button" disabled={!canUndo} onclick={onUndo}>Undo</button>
      <button type="button" disabled={!canRedo} onclick={onRedo}>Redo</button>
      <button type="button" onclick={onToggleLabels}>Labels {showEdgeLabels ? 'on' : 'off'}</button>
    </div>
    <div class="group">
      <button type="button" data-testid="graph-group" disabled={!canGroup} onclick={onGroup}>Group</button>
    </div>
    {#if scopeName}
      <nav class="breadcrumb" aria-label="Graph scope">
        <button type="button" data-testid="graph-scope-root" onclick={onExitScope}>Graph</button>
        <span>›</span>
        <strong data-testid="graph-scope-name">{scopeName}</strong>
      </nav>
    {/if}
  </div>
  <div>
    {#if dirty}
      <span class="unsaved">unsaved changes</span>
      <button type="button" class="discard" disabled={saving} onclick={onDiscard}>Discard</button>
    {/if}
    <button type="button" class="save" class:dirty data-testid="graph-save" disabled={saving} onclick={onSave}>Save profile</button>
  </div>
</div>
