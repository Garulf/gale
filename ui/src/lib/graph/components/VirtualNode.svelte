<script>
  import { getContext } from 'svelte';
  import WarningBadge from './WarningBadge.svelte';
  import { Handle, Position, useNodeConnections } from '@xyflow/svelte';
  import { snapshot } from '../../store.js';
  import { virtualInputHandles, numberedInputCount } from '../ids.js';
  import { tempValue } from '../liveValues.js';

  let { id, data, selected } = $props();

  const nodeWarnings = getContext('galeNodeWarnings');
  let warnings = $derived(nodeWarnings ? nodeWarnings()[id] || [] : []);

  let config = $derived(data.virtual.config);
  const connections = useNodeConnections({ handleType: 'target' });

  let connectedHandles = $derived(connections.current.map((c) => c.targetHandle));
  let handles = $derived(virtualInputHandles(config.type, numberedInputCount(connectedHandles) + 1));

  function inputValue(handle) {
    const connection = connections.current.find((c) => c.targetHandle === handle);
    if (!connection) return null;
    return tempValue($snapshot, connection.source, connection.sourceHandle);
  }

  function inputLabel(handle) {
    if (handle === 'in') return 'in';
    return `in ${Number(handle.slice(3)) + 1}`;
  }

  let outputValue = $derived(tempValue($snapshot, id, 'out'));
  let outputLabel = $derived.by(() => {
    if (config.type === 'mean' && config.window_s != null) return `mean · ${config.window_s}s`;
    if (config.type === 'webhook') return 'POST /api/webhook';
    return config.type;
  });
  let outputUnit = $derived(config.type === 'delta' ? '°C/min' : '°C');

  function display(value) {
    return value === null ? '—' : value.toFixed(1);
  }
</script>

<div class="node" class:sel={selected} class:warned={warnings.length > 0} data-node-id={id}>
  <h4>
    <span class="kind"></span>
    <span class="title"><span class="name">{data.virtual.name}<WarningBadge messages={warnings} /></span><small>virtual · {config.type}</small></span>
  </h4>
  <div class="rows">
    {#each handles as handle (handle)}
      {@const value = inputValue(handle)}
      <div class="row in" class:missing={value === null}>
        <Handle type="target" position={Position.Left} id={handle} class="port in" />
        <span>{inputLabel(handle)}</span>
        <span class="val temp">{display(value)}{#if value !== null}<span class="unit">°C</span>{/if}</span>
      </div>
    {/each}
    <div class="row out">
      <span>{outputLabel}</span>
      <span class="val temp">{display(outputValue)}<span class="unit">{outputUnit}</span></span>
      <Handle type="source" position={Position.Right} id="out" class="port out" />
    </div>
  </div>
</div>
