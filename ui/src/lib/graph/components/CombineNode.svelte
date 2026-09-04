<script>
  import { getContext } from 'svelte';
  import WarningBadge from './WarningBadge.svelte';
  import { Handle, Position, useNodeConnections } from '@xyflow/svelte';
  import { snapshot } from '../../store.js';
  import { dutyValue, formatDuty } from '../liveValues.js';
  import { numberedInputCount } from '../ids.js';

  let { id, data, selected } = $props();

  const nodeWarnings = getContext('galeNodeWarnings');
  let warnings = $derived(nodeWarnings ? nodeWarnings()[id] || [] : []);

  let config = $derived(data.combine.config);
  const inputConnections = useNodeConnections({ handleType: 'target' });

  let connectedHandles = $derived(inputConnections.current.map((c) => c.targetHandle));
  let handles = $derived(
    config.type === 'sync'
      ? ['in']
      : Array.from({ length: numberedInputCount(connectedHandles) + 1 }, (_, i) => `in-${i}`)
  );

  const outputConnections = useNodeConnections({ handleType: 'source', handleId: 'out' });

  function outputDuty() {
    const connection = outputConnections.current[0];
    if (!connection) return null;
    return dutyValue($snapshot, connection.targetHandle);
  }
</script>

<div class="node" class:sel={selected} class:warned={warnings.length > 0} data-node-id={id}>
  <h4>
    <span class="title">{data.combine.id}<WarningBadge messages={warnings} /></span>
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
