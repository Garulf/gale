<script>
  import { Handle, Position, useNodeConnections } from '@xyflow/svelte';
  import { snapshot } from '../../store.js';
  import { dutyValue, formatDuty } from '../liveValues.js';

  let { id, data, selected } = $props();

  let config = $derived(data.combine.config);
  let handles = $derived(
    config.type === 'sync'
      ? ['in']
      : Array.from({ length: Math.max(config.sources ? config.sources.length : 0, 1) }, (_, i) => `in-${i}`)
  );

  const outputConnections = useNodeConnections({ handleType: 'source', handleId: 'out' });

  function outputDuty() {
    const connection = outputConnections.current[0];
    if (!connection) return null;
    return dutyValue($snapshot, connection.targetHandle);
  }
</script>

<div class="node" class:sel={selected} data-node-id={id}>
  <h4>
    <span>{data.combine.id}</span>
    <small>{config.type}</small>
  </h4>
  <div class="rows">
    {#each handles as handle (handle)}
      <div class="row in">
        <Handle type="target" position={Position.Left} id={handle} class="port in duty" />
        <span>{handle}</span>
      </div>
    {/each}
  </div>
  <div class="rows">
    <div class="row out">
      <span>duty</span>
      <span class="val duty">{formatDuty(outputDuty())}</span>
      <Handle type="source" position={Position.Right} id="out" class="port out duty" />
    </div>
  </div>
</div>
