<script>
  import { getContext } from 'svelte';
  import WarningBadge from './WarningBadge.svelte';
  import { Handle, Position, useNodeConnections } from '@xyflow/svelte';
  import { snapshot } from '../../store.js';
  import { tempValue, dutyValue } from '../liveValues.js';
  import { curvePaths, curveScale, evalCurve, clamp } from '../../curveMath.js';

  const CHART_W = 210;
  const CHART_H = 64;
  const CHART_PAD = 4;

  let { id, data, selected } = $props();

  const nodeWarnings = getContext('galeNodeWarnings');
  let warnings = $derived(nodeWarnings ? nodeWarnings()[id] || [] : []);

  let config = $derived(data.curve.config);
  let hasSensorInput = $derived(config.type !== 'flat');

  const sensorConnections = useNodeConnections({ handleType: 'target', handleId: 'sensor' });
  const outputConnections = useNodeConnections({ handleType: 'source', handleId: 'out' });

  let sensorTemp = $derived.by(() => {
    const connection = sensorConnections.current[0];
    if (!connection) return null;
    return tempValue($snapshot, connection.source, connection.sourceHandle);
  });

  let outputDuty = $derived.by(() => {
    const connection = outputConnections.current[0];
    if (!connection) return null;
    return dutyValue($snapshot, connection.targetHandle);
  });

  let chart = $derived.by(() => {
    if (config.type !== 'point') return null;
    const paths = curvePaths(config.points, CHART_W, CHART_H, CHART_PAD);
    const scale = curveScale(CHART_W, CHART_H, CHART_PAD);
    const duty = sensorTemp === null ? null : evalCurve(config.points, sensorTemp);
    return {
      ...paths,
      show: sensorTemp !== null,
      dotX: sensorTemp === null ? 0 : scale.x(clamp(sensorTemp, 0, 100)),
      dotY: duty === null ? 0 : scale.y(duty),
    };
  });

  let summary = $derived.by(() => {
    if (config.type === 'flat') return `${config.duty}%`;
    if (config.type === 'trigger') return `${config.on_temp}° on · ${config.off_temp}° off`;
    if (config.type === 'target') return `hold ${config.target_temp}°`;
    return '';
  });
</script>

<div class="node duty-kind" class:sel={selected} class:warned={warnings.length > 0} data-node-id={id}>
  <h4>
    <span class="kind"></span>
    <span class="title"><span class="name">{data.curve.id}<WarningBadge messages={warnings} /></span><small>{config.type} curve</small></span>
  </h4>
  {#if hasSensorInput}
    <div class="rows">
      <div class="row in" class:missing={sensorTemp === null}>
        <Handle type="target" position={Position.Left} id="sensor" class="port in" />
        <span>sensor</span>
        <span class="val temp">{sensorTemp === null ? '—' : sensorTemp.toFixed(1)}<span class="unit">°C</span></span>
      </div>
    </div>
  {/if}
  {#if chart}
    <div class="chart">
      <svg viewBox="0 0 {CHART_W} {CHART_H}" preserveAspectRatio="none">
        <path d={chart.area} class="area" />
        <path d={chart.line} class="line" vector-effect="non-scaling-stroke" />
        {#if chart.show}
          <line x1={chart.dotX} y1="0" x2={chart.dotX} y2={CHART_H} class="live" vector-effect="non-scaling-stroke" />
          <circle cx={chart.dotX} cy={chart.dotY} r="3.5" class="dot" />
        {/if}
      </svg>
    </div>
  {/if}
  <div class="rows">
    <div class="row out">
      <span>{summary || 'duty'}</span>
      <span class="val duty">{outputDuty === null ? '—' : Math.round(outputDuty)}<span class="unit">%</span></span>
      <Handle type="source" position={Position.Right} id="out" class="port out duty" />
    </div>
  </div>
</div>
