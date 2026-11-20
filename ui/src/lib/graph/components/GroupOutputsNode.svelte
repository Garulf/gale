<script>
  import { getContext } from 'svelte';
  import { Handle, Position } from '@xyflow/svelte';
  import { snapshot } from '../../store.js';
  import { NEW_PORT_HANDLE, destinationSummary, parseGroupHandleId } from '../groups.js';
  import { tempValue, curveOutput, sensorDisplay, portState, portReading } from '../liveValues.js';
  import { nodeName } from '../ids.js';

  let { id, data } = $props();

  const endpointLabel = getContext('galeEndpointLabel');
  const curveFallback = getContext('galeCurveFallback');

  let rows = $derived(data.ports.rows);
  let expanded = $state(new Set());

  function toggleEnds(handle) {
    const next = new Set(expanded);
    if (next.has(handle)) next.delete(handle);
    else next.add(handle);
    expanded = next;
  }

  function endsId(handle) {
    return `${id}-ends-${handle}`.replace(/[^A-Za-z0-9_-]/g, '_');
  }

  function labelOf(nodeId, handle) {
    return endpointLabel ? endpointLabel(nodeId, handle) : `${nodeId} · ${handle}`;
  }

  function summary(row) {
    return destinationSummary(row.destinations || [], labelOf, row.name);
  }

  function rowValue(row) {
    if (!row.wired) return null;
    const inner = parseGroupHandleId(row.handle);
    if (row.kind === 'temp') return tempValue($snapshot, inner.nodeId, inner.handle);
    return curveOutput($snapshot, nodeName(inner.nodeId));
  }

  function rowReading(row, value) {
    if (row.kind === 'temp') return sensorDisplay(value, 'temp');
    return sensorDisplay(value, 'duty');
  }

  function isFallback(row) {
    return row.kind === 'duty' && Boolean(curveFallback) && curveFallback(nodeName(parseGroupHandleId(row.handle).nodeId));
  }
</script>

<div class="node ports-kind out" data-node-id={id}>
  <h4>
    <span class="kind"></span>
    <span class="title"><span class="name">Outputs</span></span>
  </h4>
  <div class="rows">
    {#each rows as row (row.handle)}
      {@const value = rowValue(row)}
      {@const state = portState(row.wired, value)}
      {@const reading = portReading(state, rowReading(row, value))}
      {@const described = summary(row)}
      {@const fallback = isFallback(row)}
      {@const ends = row.destinations || []}
      {@const open = expanded.has(row.handle)}
      <div class="row in {state}" class:fallback class:has-ends={ends.length > 1} title={described.title || (state === 'spare' ? 'Spare output, nothing outside reads it' : undefined)}>
        <Handle type="target" position={Position.Left} id={row.handle} class="port in {row.kind}{row.wired ? '' : ' hollow'}" />
        {#if ends.length > 1}
          <button
            type="button"
            class="ends-toggle nodrag nopan"
            aria-expanded={open}
            aria-controls={endsId(row.handle)}
            onclick={() => toggleEnds(row.handle)}
          ><span class="caret" aria-hidden="true">{open ? '▾' : '▸'}</span>{described.text}</button>
        {:else}
          <span class:unwired={!row.wired}>{described.text}</span>
        {/if}
        {#if fallback}
          <span class="fallback-tag" title="The curve has no usable input, so its fans run at the fail-safe duty"><span aria-hidden="true">!</span>Fallback</span>
        {/if}
        {#if state !== 'spare'}
          <span class="val {row.kind}">{reading.text}<span class="unit">{reading.unit}</span></span>
        {/if}
        {#if ends.length > 1 && open}
          <ul class="ends" id={endsId(row.handle)}>
            {#each ends as end (end.node + '|' + end.handle)}
              <li>{labelOf(end.node, end.handle)}</li>
            {/each}
          </ul>
        {/if}
      </div>
    {/each}
    <div class="row in new">
      <Handle type="target" position={Position.Left} id={NEW_PORT_HANDLE} class="port in hollow" />
      <span>connect to add</span>
    </div>
  </div>
</div>
