<script>
  import { getContext } from 'svelte';
  import { Handle, Position } from '@xyflow/svelte';
  import { snapshot } from '../../store.js';
  import { formatDuty } from '../liveValues.js';
  import { foldRows } from '../fold.js';

  let { id, data, selected } = $props();
  let expanded = $state(false);

  const hideNode = getContext('galeHideNode');

  let folded = $derived(foldRows(data.deviceControl.rows));
  let visibleRows = $derived(expanded ? data.deviceControl.rows : folded.visible);
  let hiddenCount = $derived(folded.hidden.length);

  function valueFor(handle) {
    const snap = $snapshot;
    const value = snap && snap.duties ? snap.duties[handle] : null;
    return formatDuty(value === undefined ? null : value);
  }
</script>

<div class="node" class:sel={selected} data-node-id={id}>
  <h4>
    <span>{data.deviceControl.device}</span>
    <button type="button" onclick={() => hideNode(id)}>Hide</button>
  </h4>
  <div class="rows">
    {#each visibleRows as row (row.handle)}
      <div class="row in">
        <Handle type="target" position={Position.Left} id={row.handle} class="port in duty" />
        <span>{row.label}</span>
        <span class="val duty">{valueFor(row.handle)}</span>
      </div>
    {/each}
  </div>
  {#if !expanded && hiddenCount > 0}
    <button type="button" class="fold" onclick={() => (expanded = true)}>+ {hiddenCount} hidden</button>
  {/if}
</div>
