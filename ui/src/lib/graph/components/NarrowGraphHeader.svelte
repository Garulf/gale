<script>
  import { useSvelteFlow } from '@xyflow/svelte';
  import AddNodeMenu from './AddNodeMenu.svelte';
  import { fitOptions } from '../viewport.js';

  let {
    view,
    onViewChange,
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

  let moreOpen = $state(false);
  let moreRoot = $state(null);

  function run(action) {
    moreOpen = false;
    action();
  }

  function closeOnOutside(event) {
    if (moreOpen && moreRoot && !moreRoot.contains(event.target)) moreOpen = false;
  }

  function closeOnEscape(event) {
    if (moreOpen && event.key === 'Escape') moreOpen = false;
  }
</script>

<svelte:window onpointerdown={closeOnOutside} onkeydown={closeOnEscape} />

<div class="gale-toolbar narrow">
  <div class="narrow-row">
    <div class="view-switch" role="group" aria-label="Graph view">
      <button type="button" aria-pressed={view === 'chains'} onclick={() => onViewChange('chains')}>Chains</button>
      <button type="button" aria-pressed={view === 'canvas'} onclick={() => onViewChange('canvas')}>Canvas</button>
    </div>
    <AddNodeMenu {onAddNode} />
    <span class="spacer"></span>
    <div class="more-menu" bind:this={moreRoot}>
      <button type="button" class="more" aria-label="More actions" aria-haspopup="menu" aria-expanded={moreOpen} onclick={() => (moreOpen = !moreOpen)}>
        <span aria-hidden="true">⋯</span>
      </button>
      {#if moreOpen}
        <div class="more-options" role="menu">
          {#if view === 'canvas'}
            <button type="button" role="menuitem" onclick={() => run(onAutoLayout)}>Auto layout</button>
            <button type="button" role="menuitem" onclick={() => run(() => fitView(fitOptions(true)))}>Fit view</button>
            <button type="button" role="menuitem" onclick={() => run(onToggleLabels)}>Labels {showEdgeLabels ? 'on' : 'off'}</button>
            <button type="button" role="menuitem" disabled={!canGroup} onclick={() => run(onGroup)}>Group</button>
          {/if}
          <button type="button" role="menuitem" disabled={!canUndo} onclick={() => run(onUndo)}>Undo</button>
          <button type="button" role="menuitem" disabled={!canRedo} onclick={() => run(onRedo)}>Redo</button>
          <button type="button" role="menuitem" disabled={!dirty || saving} onclick={() => run(onDiscard)}>Discard changes</button>
        </div>
      {/if}
    </div>
    <button type="button" class="save" class:dirty data-testid="graph-save" disabled={saving} onclick={onSave}>Save</button>
  </div>
  {#if scopeName && view === 'canvas'}
    <nav class="breadcrumb" aria-label="Graph scope">
      <button type="button" data-testid="graph-scope-root" onclick={onExitScope}>Graph</button>
      <span>›</span>
      <strong data-testid="graph-scope-name">{scopeName}</strong>
    </nav>
  {/if}
</div>
