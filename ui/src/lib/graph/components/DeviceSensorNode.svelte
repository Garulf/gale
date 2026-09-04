<script>
  import { getContext } from 'svelte';
  import WarningBadge from './WarningBadge.svelte';
  import { Handle, Position } from '@xyflow/svelte';
  import { snapshot } from '../../store.js';
  import { tempValue, formatSensor } from '../liveValues.js';
  import { foldRows } from '../fold.js';

  let { id, data, selected } = $props();

  const nodeWarnings = getContext('galeNodeWarnings');
  let warnings = $derived(nodeWarnings ? nodeWarnings()[id] || [] : []);
  let expanded = $state(false);

  const hideNode = getContext('galeHideNode');

  let folded = $derived(foldRows(data.deviceSensor.rows));
  let visibleRows = $derived(expanded ? data.deviceSensor.rows : folded.visible);
  let hiddenCount = $derived(folded.hidden.length);

  function valueFor(row) {
    return tempValue($snapshot, id, row.handle);
  }
</script>

<div class="node" class:sel={selected} class:warned={warnings.length > 0} data-node-id={id}>
  <h4>
    <span class="title">{data.deviceSensor.device}<WarningBadge messages={warnings} /></span>
    <button type="button" onclick={() => hideNode(id)}>Hide</button>
  </h4>
  <div class="rows">
    {#each visibleRows as row (row.handle)}
      <div class="row out" class:missing={valueFor(row) === null}>
        <span>{row.label}</span>
        <span class="val {row.kind || 'temp'}">{formatSensor(valueFor(row), row.kind)}</span>
        <Handle type="source" position={Position.Right} id={row.handle} class="port out" />
      </div>
    {/each}
  </div>
  {#if !expanded && hiddenCount > 0}
    <button type="button" class="fold" onclick={() => (expanded = true)}>+ {hiddenCount} hidden</button>
  {/if}
</div>
