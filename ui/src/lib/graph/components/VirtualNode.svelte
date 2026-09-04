<script>
  import { Handle, Position, useNodeConnections } from '@xyflow/svelte';
  import { snapshot } from '../../store.js';
  import { virtualInputHandles } from '../ids.js';
  import { tempValue, formatTemp } from '../liveValues.js';

  let { id, data, selected } = $props();

  let config = $derived(data.virtual.config);
  let inputCount = $derived(config.inputs ? config.inputs.length : 1);
  let handles = $derived(virtualInputHandles(config.type, inputCount));

  const connections = useNodeConnections({ handleType: 'target' });

  function inputValue(handle) {
    const connection = connections.current.find((c) => c.targetHandle === handle);
    if (!connection) return null;
    return tempValue($snapshot, connection.source, connection.sourceHandle);
  }

  let outputValue = $derived(tempValue($snapshot, id, 'out'));
</script>

<div class="node" class:sel={selected} data-node-id={id}>
  <h4>
    <span>{data.virtual.name}</span>
    <small>virtual, {config.type}</small>
  </h4>
  <div class="rows">
    {#each handles as handle (handle)}
      <div class="row in">
        <Handle type="target" position={Position.Left} id={handle} class="port in" />
        <span>{handle}</span>
        <span class="val temp">{formatTemp(inputValue(handle))}</span>
      </div>
    {/each}
    <div class="row out">
      <span>out</span>
      <span class="val temp">{formatTemp(outputValue)}</span>
      <Handle type="source" position={Position.Right} id="out" class="port out" />
    </div>
  </div>
</div>
