<script>
  import { BaseEdge, EdgeLabel, getBezierPath } from '@xyflow/svelte';
  import { snapshot } from '../../store.js';
  import { tempValue, dutyValue, formatTemp, formatDuty } from '../liveValues.js';

  let {
    id,
    source,
    sourceHandleId,
    targetHandleId,
    sourceX,
    sourceY,
    sourcePosition,
    targetX,
    targetY,
    targetPosition,
    markerStart,
    markerEnd,
    data,
  } = $props();

  let path = $derived(
    getBezierPath({ sourceX, sourceY, sourcePosition, targetX, targetY, targetPosition })
  );

  let label = $derived.by(() => {
    if (!data.showLabel) return '';
    const snap = $snapshot;
    if (data.kind === 'temp') {
      const value = tempValue(snap, source, sourceHandleId);
      return value === null ? '' : formatTemp(value);
    }
    const value = dutyValue(snap, targetHandleId);
    return value === null ? '' : formatDuty(value);
  });
</script>

<BaseEdge {id} path={path[0]} class={data.kind} {markerStart} {markerEnd} />
{#if label}
  <EdgeLabel x={path[1]} y={path[2]}>
    <div class="elabel {data.kind}">{label}</div>
  </EdgeLabel>
{/if}
