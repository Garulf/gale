<script>
  import { getContext } from 'svelte';
  import WarningBadge from './WarningBadge.svelte';
  import OverrideBadge from './OverrideBadge.svelte';
  import { Handle, Position } from '@xyflow/svelte';
  import { snapshot } from '../../store.js';
  import { dutyValue, isOverridden } from '../liveValues.js';
  import { foldRows } from '../fold.js';
  import { shortDevice } from '../../dashboard.js';

  let { id, data, selected } = $props();

  const nodeWarnings = getContext('galeNodeWarnings');
  let warnings = $derived(nodeWarnings ? nodeWarnings()[id] || [] : []);
  let expanded = $state(false);

  let folded = $derived(foldRows(data.deviceControl.rows));
  let visibleRows = $derived(expanded ? data.deviceControl.rows : folded.visible);
  let hiddenCount = $derived(folded.hidden.length);

  function valueFor(handle) {
    return dutyValue($snapshot, handle);
  }

  function rpmFor(row) {
    if (!row.tach || !$snapshot || !$snapshot.sensors) return undefined;
    const value = $snapshot.sensors[row.tach];
    return value === null || value === undefined ? null : value;
  }
</script>

<div class="node duty-kind" class:sel={selected} class:warned={warnings.length > 0} data-node-id={id}>
  <h4>
    <span class="kind"></span>
    <span class="title"><span class="name">{shortDevice(data.deviceControl.device)}<WarningBadge messages={warnings} /></span><small>device · controls</small></span>
  </h4>
  <div class="rows">
    {#each visibleRows as row (row.handle)}
      {@const value = valueFor(row.handle)}
      {@const rpm = rpmFor(row)}
      <div class="row in" class:missing={value === null}>
        <Handle type="target" position={Position.Left} id={row.handle} class="port in duty" />
        <span>{row.label}<OverrideBadge active={isOverridden($snapshot, row.handle)} /></span>
        {#if rpm !== undefined}
          <span class="rpm-tag" class:warn={rpm === null}>{rpm === null ? 'no tach' : `${Math.round(rpm)} rpm`}</span>
        {/if}
        <span class="val duty">{value === null ? '—' : Math.round(value)}<span class="unit">%</span></span>
      </div>
    {/each}
  </div>
  {#if !expanded && hiddenCount > 0}
    <button type="button" class="fold" onclick={() => (expanded = true)}>+ {hiddenCount} unwired</button>
  {/if}
</div>
