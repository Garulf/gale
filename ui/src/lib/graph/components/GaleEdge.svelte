<script>
  import { BaseEdge, EdgeLabel, getBezierPath } from '@xyflow/svelte';
  import { snapshot } from '../../store.js';
  import { tempValue, dutyValue } from '../liveValues.js';

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
      return value === null ? '' : `${value.toFixed(1)}°`;
    }
    const value = dutyValue(snap, targetHandleId);
    return value === null ? '' : `${Math.round(value)}%`;
  });
</script>

<path d={path[0]} class="edge-halo {data.kind}" />
<BaseEdge {id} path={path[0]} class={data.kind} {markerStart} {markerEnd} />
<path d={path[0]} class="edge-flow {data.kind}" />
{#if label}
  <EdgeLabel x={path[1]} y={path[2]}>
    <div class="elabel {data.kind}">{label}</div>
  </EdgeLabel>
{/if}
