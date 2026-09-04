<script>
  import { useSvelteFlow } from '@xyflow/svelte';

  let {
    showEdgeLabels,
    canUndo,
    canRedo,
    saving,
    onAutoLayout,
    onUndo,
    onRedo,
    onSave,
    onDiscard,
    onToggleLabels,
    onAddVirtual,
    onAddCurve,
    onAddCombine,
  } = $props();

  let addMenuOpen = $state(false);

  const { fitView, screenToFlowPosition } = useSvelteFlow();

  function centerPosition() {
    return screenToFlowPosition({ x: window.innerWidth / 2, y: window.innerHeight / 2 });
  }

  function addVirtual() {
    onAddVirtual(centerPosition());
    addMenuOpen = false;
  }

  function addCurve() {
    onAddCurve(centerPosition());
    addMenuOpen = false;
  }

  function addCombine() {
    onAddCombine(centerPosition());
    addMenuOpen = false;
  }
</script>

<div class="gale-toolbar">
  <div class="add-node-menu">
    <button type="button" class="primary" onclick={() => (addMenuOpen = !addMenuOpen)}>+ Node</button>
    {#if addMenuOpen}
      <div class="add-node-options">
        <button type="button" onclick={addVirtual}>Virtual sensor</button>
        <button type="button" onclick={addCurve}>Curve</button>
        <button type="button" onclick={addCombine}>Combine</button>
      </div>
    {/if}
  </div>
  <button type="button" onclick={onAutoLayout}>Auto layout</button>
  <button type="button" onclick={() => fitView()}>Fit view</button>
  <button type="button" disabled={!canUndo} onclick={onUndo}>Undo</button>
  <button type="button" disabled={!canRedo} onclick={onRedo}>Redo</button>
  <button type="button" data-testid="graph-save" disabled={saving} onclick={onSave}>Save</button>
  <button type="button" disabled={saving} onclick={onDiscard}>Discard</button>
  <button type="button" onclick={onToggleLabels}>Edge labels: {showEdgeLabels ? 'on' : 'off'}</button>
</div>

<style>
  .add-node-menu {
    position: relative;
  }

  .add-node-options {
    position: absolute;
    top: 100%;
    left: 0;
    margin-top: 4px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    background: var(--panel);
    border: 1px solid var(--node-edge);
    border-radius: 6px;
    padding: 4px;
    z-index: 10;
  }
</style>
