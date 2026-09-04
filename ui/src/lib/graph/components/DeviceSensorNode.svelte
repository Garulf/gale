<script>
  import { getContext } from 'svelte';
  import { Handle, Position } from '@xyflow/svelte';
  import { snapshot } from '../../store.js';
  import { formatTemp } from '../liveValues.js';

  let { id, data, selected } = $props();
  let expanded = $state(false);

  const hideNode = getContext('galeHideNode');

  let visibleRows = $derived(expanded ? data.deviceSensor.rows : data.deviceSensor.rows.slice(0, 3));
  let hiddenCount = $derived(Math.max(0, data.deviceSensor.rows.length - 3));

  function valueFor(handle) {
    const snap = $snapshot;
    const value = snap && snap.sensors ? snap.sensors[handle] : null;
    return formatTemp(value === undefined ? null : value);
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
        <span class="val temp">{valueFor(row.handle)}</span>
        <Handle type="source" position={Position.Right} id={row.handle} class="port out" />
      </div>
    {/each}
  </div>
  {#if !expanded && hiddenCount > 0}
    <button type="button" class="fold" onclick={() => (expanded = true)}>+ {hiddenCount} hidden</button>
  {/if}
</div>
