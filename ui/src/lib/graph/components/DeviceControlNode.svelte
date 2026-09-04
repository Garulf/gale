<script>
  import { getContext } from 'svelte';
  import WarningBadge from './WarningBadge.svelte';
  import OverrideBadge from './OverrideBadge.svelte';
  import { Handle, Position } from '@xyflow/svelte';
  import { snapshot } from '../../store.js';
  import { dutyValue, formatDuty, isOverridden } from '../liveValues.js';
  import { foldRows } from '../fold.js';

  let { id, data, selected } = $props();

  const nodeWarnings = getContext('galeNodeWarnings');
  let warnings = $derived(nodeWarnings ? nodeWarnings()[id] || [] : []);
  let expanded = $state(false);

  const hideNode = getContext('galeHideNode');

  let folded = $derived(foldRows(data.deviceControl.rows));
  let visibleRows = $derived(expanded ? data.deviceControl.rows : folded.visible);
  let hiddenCount = $derived(folded.hidden.length);

  function valueFor(handle) {
    return dutyValue($snapshot, handle);
  }

  function overriddenFor(handle) {
    return isOverridden($snapshot, handle);
  }
</script>

<div class="node" class:sel={selected} class:warned={warnings.length > 0} data-node-id={id}>
  <h4>
    <span class="title">{data.deviceControl.device}<WarningBadge messages={warnings} /></span>
    <button type="button" onclick={() => hideNode(id)}>Hide</button>
  </h4>
  <div class="rows">
    {#each visibleRows as row (row.handle)}
      <div class="row in" class:missing={valueFor(row.handle) === null}>
        <Handle type="target" position={Position.Left} id={row.handle} class="port in duty" />
        <span>{row.label}<OverrideBadge active={overriddenFor(row.handle)} /></span>
        <span class="val duty">{formatDuty(valueFor(row.handle))}</span>
      </div>
    {/each}
  </div>
  {#if !expanded && hiddenCount > 0}
    <button type="button" class="fold" onclick={() => (expanded = true)}>+ {hiddenCount} hidden</button>
  {/if}
</div>
