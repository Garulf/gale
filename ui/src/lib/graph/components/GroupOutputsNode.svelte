<script>
  import { getContext } from 'svelte';
  import { Handle, Position } from '@xyflow/svelte';
  import { NEW_PORT_HANDLE } from '../groups.js';

  let { id, data } = $props();

  const endpointLabel = getContext('galeEndpointLabel');

  let rows = $derived(data.ports.rows);

  function rowLabel(row) {
    if (row.wired && row.outside && endpointLabel) return endpointLabel(row.outside.node, row.outside.handle);
    return row.name || 'unwired';
  }
</script>

<div class="node ports-kind out" data-node-id={id}>
  <h4>
    <span class="kind"></span>
    <span class="title"><span class="name">Outputs</span></span>
  </h4>
  <div class="rows">
    {#each rows as row (row.handle)}
      <div class="row in" class:unwired={!row.wired}>
        <Handle type="target" position={Position.Left} id={row.handle} class="port in {row.kind}{row.wired ? '' : ' hollow'}" />
        <span>{rowLabel(row)}</span>
      </div>
    {/each}
    <div class="row in new">
      <Handle type="target" position={Position.Left} id={NEW_PORT_HANDLE} class="port in hollow" />
      <span>connect to add</span>
    </div>
  </div>
</div>
