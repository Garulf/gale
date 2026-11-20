<script>
  import { getContext } from 'svelte';
  import { Handle, Position } from '@xyflow/svelte';
  import { snapshot } from '../../store.js';
  import { NEW_PORT_HANDLE } from '../groups.js';
  import { tempValue, curveOutput, sensorDisplay, portState } from '../liveValues.js';
  import { nodeName } from '../ids.js';

  let { id, data } = $props();

  const endpointLabel = getContext('galeEndpointLabel');
  const sensorKind = getContext('galeSensorKind');

  let rows = $derived(data.ports.rows);

  function rowLabel(row) {
    if (row.wired && row.outside && endpointLabel) return endpointLabel(row.outside.node, row.outside.handle);
    return row.name || 'spare';
  }

  function rowValue(row) {
    if (!row.wired || !row.outside) return null;
    if (row.kind === 'duty') return curveOutput($snapshot, nodeName(row.outside.node));
    return tempValue($snapshot, row.outside.node, row.outside.handle);
  }

  function rowReading(row, value) {
    if (row.kind === 'duty') return sensorDisplay(value, 'duty');
    const kind = sensorKind && row.outside ? sensorKind(row.outside.node, row.outside.handle) : undefined;
    return sensorDisplay(value, kind);
  }

  function rowTitle(state) {
    if (state === 'spare') return 'Spare input, nothing is wired into it';
    if (state === 'unavailable') return 'Wired, but no live reading';
    return undefined;
  }
</script>

<div class="node ports-kind in" data-node-id={id}>
  <h4>
    <span class="kind"></span>
    <span class="title"><span class="name">Inputs</span></span>
  </h4>
  <div class="rows">
    {#each rows as row (row.handle)}
      {@const value = rowValue(row)}
      {@const state = portState(row.wired, value)}
      {@const reading = rowReading(row, value)}
      <div class="row out {state}" title={rowTitle(state)}>
        <span class:unwired={!row.wired}>{rowLabel(row)}</span>
        {#if state !== 'spare'}
          <span class="val {row.kind}">{reading.text}<span class="unit">{reading.unit}</span></span>
        {/if}
        <Handle type="source" position={Position.Right} id={row.handle} class="port out {row.kind}{row.wired ? '' : ' hollow'}" />
      </div>
    {/each}
    <div class="row out new">
      <span>connect to add</span>
      <Handle type="source" position={Position.Right} id={NEW_PORT_HANDLE} class="port out hollow" />
    </div>
  </div>
</div>
