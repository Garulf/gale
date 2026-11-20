<script>
  import { getContext } from 'svelte';
  import WarningBadge from './WarningBadge.svelte';
  import { Handle, Position, useNodeConnections } from '@xyflow/svelte';
  import { snapshot } from '../../store.js';
  import { dutyValue, curveOutput } from '../liveValues.js';
  import { numberedInputCount, isSingleInputCombineType } from '../ids.js';
  import { operationLabel } from '../palette.js';

  let { id, data, selected } = $props();

  const nodeWarnings = getContext('galeNodeWarnings');
  const curveFallback = getContext('galeCurveFallback');
  const resolveTargets = getContext('galeResolveTargets');
  let fallback = $derived(curveFallback ? curveFallback(data.combine.id) : false);
  let warnings = $derived(nodeWarnings ? nodeWarnings()[id] || [] : []);

  let config = $derived(data.combine.config);
  const inputConnections = useNodeConnections({ handleType: 'target' });

  let connectedHandles = $derived(inputConnections.current.map((c) => c.targetHandle));
  let handles = $derived(
    isSingleInputCombineType(config.type)
      ? ['in']
      : Array.from({ length: numberedInputCount(connectedHandles) + 1 }, (_, i) => `in-${i}`)
  );

  function sourceName(handle) {
    const connection = inputConnections.current.find((c) => c.targetHandle === handle);
    if (!connection) return handle === 'in' ? 'in' : `in ${Number(handle.slice(3)) + 1}`;
    return connection.source.slice(connection.source.indexOf(':') + 1);
  }

  const outputConnections = useNodeConnections({ handleType: 'source', handleId: 'out' });

  let outputDuty = $derived.by(() => {
    const published = curveOutput($snapshot, data.combine.id);
    if (published !== null) return published;
    const connection = outputConnections.current[0];
    if (!connection) return null;
    const targets = resolveTargets ? resolveTargets(connection.target, connection.targetHandle) : [{ node: connection.target, handle: connection.targetHandle }];
    return targets.length > 0 ? dutyValue($snapshot, targets[0].handle) : null;
  });

  let outputLabel = $derived(config.type === 'mix' ? `${config.mode} of inputs` : config.type === 'offset' ? `× ${config.scale} + ${config.add}` : 'follows input');
</script>

<div class="node duty-kind" class:sel={selected} class:warned={warnings.length > 0} data-node-id={id}>
  <h4>
    <span class="kind"></span>
    <span class="title"><span class="name">{data.combine.id}<WarningBadge messages={warnings} /></span><small>{operationLabel('combine', config)}</small></span>
  </h4>
  <div class="rows">
    {#each handles as handle (handle)}
      {@const wired = connectedHandles.includes(handle)}
      <div class="row in" class:missing={!wired}>
        <Handle type="target" position={Position.Left} id={handle} class="port in duty" />
        <span>{sourceName(handle)}</span>
      </div>
    {/each}
    <div class="row out" class:fallback>
      <span>{outputLabel}</span>
      {#if fallback}
        <span class="fallback-tag" title="This curve has no usable input, so its fans run at the fail-safe duty"><span aria-hidden="true">!</span>Fallback</span>
      {/if}
      <span class="val duty">{outputDuty === null ? '—' : Math.round(outputDuty)}<span class="unit">%</span></span>
      <Handle type="source" position={Position.Right} id="out" class="port out duty" />
    </div>
  </div>
</div>
