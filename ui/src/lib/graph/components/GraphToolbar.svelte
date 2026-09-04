<script>
  import { useSvelteFlow } from '@xyflow/svelte';

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
  <div>
    <div class="add-node-menu">
      <button type="button" onclick={() => (addMenuOpen = !addMenuOpen)}>+ Node</button>
      {#if addMenuOpen}
        <div class="add-node-options">
          <button type="button" data-testid="add-node-virtual" onclick={addVirtual}><strong>Virtual sensor</strong><small>max · min · mean · offset · delta · webhook</small></button>
          <button type="button" onclick={addCurve}><strong>Curve</strong><small>point · trigger · target · flat</small></button>
          <button type="button" onclick={addCombine}><strong>Combine</strong><small>mix · sync</small></button>
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
