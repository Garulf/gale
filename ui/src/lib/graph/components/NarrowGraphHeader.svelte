<script>
  import { tick } from 'svelte';
  import { useSvelteFlow } from '@xyflow/svelte';
  import AddNodeMenu from './AddNodeMenu.svelte';
  import { fitOptions } from '../viewport.js';
  import { nextMenuIndex } from '../menuNav.js';

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
  let moreButton = $state(null);
  let moreMenu = $state(null);

  function menuItems() {
    return moreMenu ? [...moreMenu.querySelectorAll('[role="menuitem"]')] : [];
  }

  function focusItem(key) {
    const items = menuItems();
    const index = nextMenuIndex(items.map((item) => !item.disabled), items.indexOf(document.activeElement), key);
    if (index < 0) return false;
    items[index].focus();
    return true;
  }

  async function openMenu(key = 'Home') {
    moreOpen = true;
    await tick();
    if (!focusItem(key) && moreMenu) moreMenu.focus();
  }

  function closeMenu() {
    moreOpen = false;
    if (moreButton) moreButton.focus();
  }

  function toggleMenu() {
    if (moreOpen) closeMenu();
    else openMenu();
  }

  function onMoreKey(event) {
    if (moreOpen || (event.key !== 'ArrowDown' && event.key !== 'ArrowUp')) return;
    event.preventDefault();
    openMenu(event.key === 'ArrowUp' ? 'End' : 'Home');
  }

  function onMenuKey(event) {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      closeMenu();
      return;
    }
    if (event.key === 'Tab') {
      closeMenu();
      return;
    }
    if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    focusItem(event.key);
  }

  function run(action) {
    closeMenu();
    action();
  }

  function closeOnOutside(event) {
    if (moreOpen && moreRoot && !moreRoot.contains(event.target)) moreOpen = false;
  }
</script>

<svelte:window onpointerdown={closeOnOutside} />

<div class="gale-toolbar narrow">
  <div class="narrow-row">
    <div class="view-switch" role="group" aria-label="Graph view">
      <button type="button" aria-pressed={view === 'chains'} onclick={() => onViewChange('chains')}>Chains</button>
      <button type="button" aria-pressed={view === 'canvas'} onclick={() => onViewChange('canvas')}>Canvas</button>
    </div>
    <AddNodeMenu {onAddNode} />
    <span class="spacer"></span>
    <div class="more-menu" bind:this={moreRoot}>
      <button type="button" class="more" aria-label="More actions" aria-haspopup="menu" aria-expanded={moreOpen} aria-controls="gale-more-menu" bind:this={moreButton} onclick={toggleMenu} onkeydown={onMoreKey}>
        <span aria-hidden="true">⋯</span>
      </button>
      {#if moreOpen}
        <div class="more-options" id="gale-more-menu" role="menu" aria-label="More actions" tabindex="-1" bind:this={moreMenu} onkeydown={onMenuKey}>
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
    <button type="button" class="save" class:dirty data-testid="graph-save" aria-label={dirty ? 'Save, unsaved changes' : 'Save'} disabled={saving} onclick={onSave}>Save</button>
  </div>
  {#if scopeName && view === 'canvas'}
    <nav class="breadcrumb" aria-label="Graph scope">
      <button type="button" data-testid="graph-scope-root" onclick={onExitScope}>Graph</button>
      <span>›</span>
      <strong data-testid="graph-scope-name">{scopeName}</strong>
    </nav>
  {/if}
</div>
