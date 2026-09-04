<script>
  import { snapshot } from '../../store.js';
  import { tempValue, dutyValue, isOverridden } from '../liveValues.js';
  import { buildChains, chainDevices, chainMatchesFilter, stepKey } from '../chains.js';
  import { curvePaths, curveScale, evalCurve, clamp } from '../../curveMath.js';
  import { shortDevice } from '../../dashboard.js';

  const CHART_W = 220;
  const CHART_H = 84;
  const CHART_PAD = 6;

  let { nodes, edges, selectedNodeId, onSelect } = $props();

  let filter = $state('all');
  let chains = $derived(buildChains(nodes, edges));
  let devices = $derived(chainDevices(chains, nodes));
  let overrides = $derived(($snapshot && $snapshot.overrides) || []);
  let filters = $derived([['all', 'All chains'], ...devices.map((device) => [device, device]), ['manual', 'Manual']]);
  let visible = $derived(chains.filter((chain) => chainMatchesFilter(chain, nodes, filter, overrides)));

  function nodeById(id) {
    return nodes.find((candidate) => candidate.id === id);
  }

  function fmt1(value) {
    return value === null || value === undefined ? '—' : value.toFixed(1);
  }

  function describe(step) {
    const node = nodeById(step.nodeId);
    if (!node) return null;
    const snap = $snapshot;
    if (node.type === 'deviceSensor') {
      const row = node.data.deviceSensor.rows.find((candidate) => candidate.handle === step.handle);
      const value = tempValue(snap, node.id, step.handle);
      const rpm = row && row.kind === 'rpm';
      return { title: row ? row.label : step.handle, sub: shortDevice(node.data.deviceSensor.device), value: value === null ? '—' : rpm ? String(Math.round(value)) : value.toFixed(1), unit: rpm ? 'rpm' : '°C', kind: 'temp', editable: true, warned: value === null };
    }
    if (node.type === 'deviceControl') {
      const row = node.data.deviceControl.rows.find((candidate) => candidate.handle === step.handle);
      const value = dutyValue(snap, step.handle);
      const rpm = row && row.tach && snap && snap.sensors ? snap.sensors[row.tach] : undefined;
      const manual = isOverridden(snap, step.handle);
      return { title: (row ? row.label : step.handle) + (manual ? ' · M' : ''), sub: `${shortDevice(node.data.deviceControl.device)}${rpm === undefined ? '' : rpm === null ? ' · no tach' : ` · ${Math.round(rpm)} rpm`}`, value: value === null ? '—' : String(Math.round(value)), unit: '%', kind: 'duty', editable: true, warned: rpm === null };
    }
    if (node.type === 'virtual') {
      const config = node.data.virtual.config;
      const value = tempValue(snap, node.id, 'out');
      return { title: node.data.virtual.name, sub: `virtual · ${config.type}${config.window_s ? ` · ${config.window_s}s` : ''}`, value: fmt1(value), unit: config.type === 'delta' ? '°C/min' : '°C', kind: 'temp', editable: true };
    }
    if (node.type === 'curve') {
      const config = node.data.curve.config;
      const sensorEdge = edges.find((edge) => edge.target === node.id && edge.targetHandle === 'sensor');
      const temp = sensorEdge ? tempValue(snap, sensorEdge.source, sensorEdge.sourceHandle) : null;
      const controlEdge = edges.find((edge) => edge.source === node.id && edge.target.startsWith('control:'));
      const duty = controlEdge ? dutyValue(snap, controlEdge.targetHandle) : null;
      let chart = null;
      if (config.type === 'point') {
        const paths = curvePaths(config.points, CHART_W, CHART_H, CHART_PAD);
        const scale = curveScale(CHART_W, CHART_H, CHART_PAD);
        const liveDuty = temp === null ? null : evalCurve(config.points, temp);
        chart = { ...paths, show: temp !== null, dotX: temp === null ? 0 : scale.x(clamp(temp, 0, 100)), dotY: liveDuty === null ? 0 : scale.y(liveDuty) };
      }
      return { title: node.data.curve.id, sub: `${config.type} curve`, value: duty === null ? '—' : String(Math.round(duty)), unit: '%', kind: 'duty', editable: true, chart };
    }
    if (node.type === 'combine') {
      const config = node.data.combine.config;
      const controlEdge = edges.find((edge) => edge.source === node.id && edge.target.startsWith('control:'));
      const duty = controlEdge ? dutyValue(snap, controlEdge.targetHandle) : null;
      return { title: node.data.combine.id, sub: `combine · ${config.type}${config.mode ? ` · ${config.mode}` : ''}`, value: duty === null ? '—' : String(Math.round(duty)), unit: '%', kind: 'duty', editable: true };
    }
    return null;
  }
</script>

<div class="chain-view">
  <div class="filters">
    {#each filters as [key, label] (key)}
      <button type="button" class:on={filter === key} onclick={() => (filter = key)}>{label}</button>
    {/each}
  </div>
  {#each visible as chain (chain.id)}
    <div class="chain" class:unassigned={chain.unassigned}>
      {#each chain.steps as step, index (stepKey(step))}
        {@const info = describe(step)}
        {#if info}
          {@const prev = index > 0 ? describe(chain.steps[index - 1]) : null}
          <div class="step">
            <div class="rail">
              <span class="line" class:temp={prev && prev.kind === 'temp'} class:duty={prev && prev.kind === 'duty'} class:hidden={index === 0}></span>
              <span class="port {info.kind}"></span>
              <span class="line {info.kind}" class:hidden={index === chain.steps.length - 1}></span>
            </div>
            <button type="button" class="node-card" class:sel={selectedNodeId === step.nodeId} class:warned={info.warned} onclick={() => onSelect(step.nodeId)}>
              <div class="node-head">
                <span class="kind {info.kind}"></span>
                <span class="text"><span class="title">{info.title}</span><span class="mono sub">{info.sub}</span></span>
                <span class="mono val {info.kind}">{info.value}<span class="unit">{info.unit}</span></span>
                <span class="chev">›</span>
              </div>
              {#if info.chart}
                <div class="chart">
                  <svg viewBox="0 0 {CHART_W} {CHART_H}" preserveAspectRatio="none">
                    <path d={info.chart.area} class="area" />
                    <path d={info.chart.line} class="line-path" vector-effect="non-scaling-stroke" />
                    {#if info.chart.show}
                      <line x1={info.chart.dotX} y1="0" x2={info.chart.dotX} y2={CHART_H} class="live" vector-effect="non-scaling-stroke" />
                      <circle cx={info.chart.dotX} cy={info.chart.dotY} r="5" class="dot" />
                    {/if}
                  </svg>
                </div>
              {/if}
            </button>
          </div>
        {/if}
      {/each}
      {#if chain.unassigned}
        <span class="mono unassigned-tag">not driving any fan</span>
      {/if}
    </div>
  {/each}
  {#if visible.length === 0}
    <p class="muted">No chains match this filter.</p>
  {/if}
</div>

<style>
  .chain-view {
    display: flex;
    flex-direction: column;
    gap: 18px;
  }

  .filters {
    display: flex;
    gap: 6px;
    overflow-x: auto;
    padding-bottom: 2px;
    flex-shrink: 0;
  }

  .filters button {
    height: 32px;
    padding: 0 12px;
    border: 1px solid var(--line);
    border-radius: 999px;
    background: none;
    color: var(--muted);
    font-size: 12px;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .filters button.on {
    border-color: var(--accent);
    background: var(--tempbg);
    color: var(--ink);
  }

  .chain {
    display: flex;
    flex-direction: column;
  }

  .step {
    display: flex;
    gap: 12px;
    align-items: stretch;
  }

  .rail {
    width: 14px;
    display: flex;
    flex-direction: column;
    align-items: center;
    flex-shrink: 0;
  }

  .rail .line {
    width: 2px;
    flex: 1;
    background: var(--line);
  }

  .rail .line.temp {
    background: var(--temp);
  }

  .rail .line.duty {
    background: var(--duty);
  }

  .rail .line.hidden {
    background: transparent;
  }

  .port {
    width: 11px;
    height: 11px;
    border-radius: 50%;
    border: 2px solid var(--canvas);
    flex-shrink: 0;
  }

  .port.temp {
    background: var(--temp);
    box-shadow: 0 0 0 1px var(--temp);
  }

  .port.duty {
    background: var(--duty);
    box-shadow: 0 0 0 1px var(--duty);
  }

  .node-card {
    flex: 1;
    margin: 5px 0;
    background: var(--node);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    min-height: 56px;
    display: flex;
    flex-direction: column;
    padding: 0;
    color: var(--ink);
    text-align: left;
    min-width: 0;
  }

  .node-card.sel {
    border-color: var(--accent);
    box-shadow: var(--glow);
  }

  .node-card.warned {
    border-color: var(--warn);
  }

  .node-head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    min-height: 56px;
  }

  .kind {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    flex-shrink: 0;
  }

  .kind.temp,
  .val.temp {
    color: var(--temp);
  }

  .kind.temp {
    background: var(--temp);
  }

  .kind.duty {
    background: var(--duty);
  }

  .val.duty {
    color: var(--duty);
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 1;
  }

  .title {
    font-weight: 600;
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .sub {
    font-size: 10px;
    color: var(--muted);
  }

  .val {
    font-size: 15px;
    font-weight: 500;
  }

  .unit {
    font-size: 10px;
    color: var(--muted);
    margin-left: 2px;
  }

  .chev {
    color: var(--faint);
    font-size: 16px;
  }

  .chart {
    margin: 0 12px 10px;
    background: var(--canvas);
    border: 1px solid var(--line);
    border-radius: var(--r);
    overflow: hidden;
  }

  .chart svg {
    width: 100%;
    height: 56px;
    display: block;
  }

  .chart .area {
    fill: var(--dutybg);
  }

  .chart .line-path {
    fill: none;
    stroke: var(--duty);
    stroke-width: 1.5;
  }

  .chart .live {
    stroke: var(--temp);
    stroke-dasharray: 3 3;
  }

  .chart .dot {
    fill: var(--temp);
  }

  .unassigned-tag {
    font-size: 10px;
    color: var(--faint);
    padding-left: 26px;
  }
</style>
