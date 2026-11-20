<script>
  import { getContext } from 'svelte';
  import { BaseEdge, EdgeLabel, getBezierPath } from '@xyflow/svelte';
  import { snapshot } from '../../store.js';
  import { tempValue, dutyValue, sensorDisplay } from '../liveValues.js';
  import { nodeKind } from '../ids.js';
  import { axisMark } from '../../units.js';

  let {
    id,
    source,
    target,
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

  const edgeSaved = getContext('galeEdgeSaved');
  const sensorKind = getContext('galeSensorKind');
  const resolveSource = getContext('galeResolveSource');
  const resolveTargets = getContext('galeResolveTargets');

  function origin() {
    return resolveSource ? resolveSource(source, sourceHandleId) : { node: source, handle: sourceHandleId };
  }

  function destinations() {
    return resolveTargets ? resolveTargets(target, targetHandleId) : [{ node: target, handle: targetHandleId }];
  }

  function dutyLabelValue(snap) {
    const control = destinations().find((endpoint) => nodeKind(endpoint.node) === 'control');
    return control ? dutyValue(snap, control.handle) : null;
  }
  let saved = $derived(edgeSaved ? edgeSaved(id) : true);

  let path = $derived(
    getBezierPath({ sourceX, sourceY, sourcePosition, targetX, targetY, targetPosition })
  );

  let label = $derived.by(() => {
    if (!data.showLabel || !saved || data.declared) return '';
    const snap = $snapshot;
    if (data.kind === 'temp') {
      const from = origin();
      if (!from) return '';
      const value = tempValue(snap, from.node, from.handle);
      if (value === null) return '';
      const kind = sensorKind ? sensorKind(from.node, from.handle) : undefined;
      const reading = sensorDisplay(value, kind);
      const mark = axisMark(reading.unit);
      if (mark === '°') return `${reading.text}°`;
      return mark ? `${reading.text} ${mark}` : reading.text;
    }
    const value = dutyLabelValue(snap);
    return value === null ? '' : `${Math.round(value)}%`;
  });
</script>

{#if data.declared}
  <BaseEdge {id} path={path[0]} class="{data.kind} declared" {markerStart} {markerEnd} />
{:else}
  <path d={path[0]} class="edge-halo {data.kind}" />
  <BaseEdge {id} path={path[0]} class={data.kind} {markerStart} {markerEnd} />
  <path d={path[0]} class="edge-flow {data.kind}" />
{/if}
{#if label}
  <EdgeLabel x={path[1]} y={path[2]}>
    <div class="elabel {data.kind}" class:edge-dim={data.emphasis === 'dim'}>{label}</div>
  </EdgeLabel>
{/if}
