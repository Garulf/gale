<script>
  import { Handle, Position, useNodeConnections } from '@xyflow/svelte';
  import { snapshot } from '../../store.js';
  import { tempValue, dutyValue, formatTemp, formatDuty } from '../liveValues.js';
  import { sparkPath } from '../sparkline.js';

  let { id, data, selected } = $props();

  let config = $derived(data.curve.config);
  let hasSensorInput = $derived(config.type !== 'flat');

  const sensorConnections = useNodeConnections({ handleType: 'target', handleId: 'sensor' });
  const outputConnections = useNodeConnections({ handleType: 'source', handleId: 'out' });

  function sensorValue() {
    const connection = sensorConnections.current[0];
    if (!connection) return null;
    return tempValue($snapshot, connection.source, connection.sourceHandle);
  }

  function outputDuty() {
    const connection = outputConnections.current[0];
    if (!connection) return null;
    return dutyValue($snapshot, connection.targetHandle);
  }

  let path = $derived(config.type === 'point' ? sparkPath(config.points, 132, 44) : '');
</script>

<div class="node" class:sel={selected} data-node-id={id}>
  <h4>
    <span>{data.curve.id}</span>
    <small>{config.type}</small>
  </h4>
  {#if hasSensorInput}
    <div class="rows">
      <div class="row io">
        <Handle type="target" position={Position.Left} id="sensor" class="port in" />
        <span>sensor</span>
        <span class="val temp">{formatTemp(sensorValue())}</span>
      </div>
    </div>
  {/if}
  {#if config.type === 'point'}
    <div class="spark">
      <svg width="132" height="44" viewBox="0 0 132 44">
        <path d={path} fill="none" stroke="#4c9ff0" stroke-width="2" />
      </svg>
    </div>
  {/if}
  <div class="rows">
    <div class="row out">
      <span>duty</span>
      <span class="val duty">{formatDuty(outputDuty())}</span>
      <Handle type="source" position={Position.Right} id="out" class="port out duty" />
    </div>
  </div>
</div>
