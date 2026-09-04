<script>
  import { getContext } from 'svelte';
  import WarningBadge from './WarningBadge.svelte';
  import { Handle, Position } from '@xyflow/svelte';
  import { snapshot } from '../../store.js';
  import { tempValue } from '../liveValues.js';
  import { foldRows } from '../fold.js';
  import { shortDevice } from '../../dashboard.js';

  let { id, data, selected } = $props();

  const nodeWarnings = getContext('galeNodeWarnings');
  let warnings = $derived(nodeWarnings ? nodeWarnings()[id] || [] : []);
  let expanded = $state(false);

  let folded = $derived(foldRows(data.deviceSensor.rows));
  let visibleRows = $derived(expanded ? data.deviceSensor.rows : folded.visible);
  let hiddenCount = $derived(folded.hidden.length);

  function valueFor(row) {
    return tempValue($snapshot, id, row.handle);
  }

  function display(value, kind) {
    if (value === null) return '—';
    return kind === 'rpm' ? String(Math.round(value)) : value.toFixed(1);
  }

  function unit(kind) {
    return kind === 'rpm' ? 'rpm' : '°C';
  }
</script>

<div class="node" class:sel={selected} class:warned={warnings.length > 0} data-node-id={id}>
  <h4>
    <span class="kind"></span>
    <span class="title"><span class="name">{shortDevice(data.deviceSensor.device)}<WarningBadge messages={warnings} /></span><small>device · sensors</small></span>
  </h4>
  <div class="rows">
    {#each visibleRows as row (row.handle)}
      {@const value = valueFor(row)}
      <div class="row out" class:missing={value === null}>
        <span>{row.label}</span>
        <span class="val {row.kind || 'temp'}">{display(value, row.kind)}<span class="unit">{unit(row.kind)}</span></span>
        <Handle type="source" position={Position.Right} id={row.handle} class="port out" />
      </div>
    {/each}
  </div>
  {#if !expanded && hiddenCount > 0}
    <button type="button" class="fold" onclick={() => (expanded = true)}>+ {hiddenCount} unwired</button>
  {/if}
</div>
