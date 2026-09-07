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

<div class="node ports-kind in" data-node-id={id}>
  <h4>
    <span class="kind"></span>
    <span class="title"><span class="name">Inputs</span></span>
  </h4>
  <div class="rows">
    {#each rows as row (row.handle)}
      <div class="row out">
        <span class:unwired={!row.wired}>{rowLabel(row)}</span>
        <Handle type="source" position={Position.Right} id={row.handle} class="port out {row.kind}{row.wired ? '' : ' hollow'}" />
      </div>
    {/each}
    <div class="row out new">
      <span>connect to add</span>
      <Handle type="source" position={Position.Right} id={NEW_PORT_HANDLE} class="port out hollow" />
    </div>
  </div>
</div>
