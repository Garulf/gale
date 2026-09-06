<script>
  import { Handle, Position } from '@xyflow/svelte';
  import { snapshot } from '../../store.js';
  import { tempValue, dutyValue, sensorDisplay } from '../liveValues.js';

  let { id, data } = $props();

  let port = $derived(data.port);
  let reading = $derived.by(() => {
    if (port.direction === 'in') {
      const value = port.kind === 'temp' ? tempValue($snapshot, port.nodeId, port.handle) : null;
      return port.kind === 'temp' ? sensorDisplay(value, 'temp') : { text: '', unit: '' };
    }
    const value = dutyValue($snapshot, port.handle);
    return { text: value === null ? '—' : String(Math.round(value)), unit: '%' };
  });
</script>

<div class="node port-kind {port.direction}" data-node-id={id}>
  <h4>
    <span class="kind"></span>
    <span class="title"><span class="name">{port.label}</span><small>{port.direction === 'in' ? 'from outside' : 'to outside'}</small></span>
  </h4>
  <div class="rows">
    <div class="row {port.direction === 'in' ? 'out' : 'in'}">
      {#if port.direction === 'out'}
        <Handle type="target" position={Position.Left} id="port" class="port in {port.kind}" />
      {/if}
      <span>{port.kind}</span>
      <span class="val {port.kind}">{reading.text}<span class="unit">{reading.unit}</span></span>
      {#if port.direction === 'in'}
        <Handle type="source" position={Position.Right} id="port" class="port out {port.kind}" />
      {/if}
    </div>
  </div>
</div>
