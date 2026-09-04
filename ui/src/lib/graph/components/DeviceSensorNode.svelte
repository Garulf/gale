<script>
  import { getContext } from 'svelte';
  import { Handle, Position } from '@xyflow/svelte';
  import { snapshot } from '../../store.js';
  import { formatSensor } from '../liveValues.js';
  import { foldRows } from '../fold.js';

  let { id, data, selected } = $props();
  let expanded = $state(false);

  const hideNode = getContext('galeHideNode');

  let folded = $derived(foldRows(data.deviceSensor.rows));
  let visibleRows = $derived(expanded ? data.deviceSensor.rows : folded.visible);
  let hiddenCount = $derived(folded.hidden.length);

  function valueFor(row) {
    const snap = $snapshot;
    const value = snap && snap.sensors ? snap.sensors[row.handle] : null;
    return formatSensor(value === undefined ? null : value, row.kind);
  }
</script>

<div class="node" class:sel={selected} data-node-id={id}>
  <h4>
    <span>{data.deviceSensor.device}</span>
    <button type="button" onclick={() => hideNode(id)}>Hide</button>
  </h4>
  <div class="rows">
    {#each visibleRows as row (row.handle)}
      <div class="row out">
        <span>{row.label}</span>
        <span class="val {row.kind || 'temp'}">{valueFor(row)}</span>
        <Handle type="source" position={Position.Right} id={row.handle} class="port out" />
      </div>
    {/each}
  </div>
  {#if !expanded && hiddenCount > 0}
    <button type="button" class="fold" onclick={() => (expanded = true)}>+ {hiddenCount} hidden</button>
  {/if}
</div>
