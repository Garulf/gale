<script>
  import { useSvelteFlow } from '@xyflow/svelte';
  import AddNodeMenu from './AddNodeMenu.svelte';
  import { fitOptions } from '../viewport.js';

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
    onExitScope,
    canGroup = false,
    onGroup,
  } = $props();

  const { fitView } = useSvelteFlow();
</script>

<div class="gale-toolbar">
  <div>
    <AddNodeMenu {onAddNode} />
    <div class="group">
      <button type="button" onclick={onAutoLayout}>Auto layout</button>
      <button type="button" onclick={() => fitView(fitOptions(false))}>Fit view</button>
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
