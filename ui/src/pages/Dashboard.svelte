<script>
  import { onMount } from 'svelte';
  import { snapshot } from '../lib/store.js';
  import { getInventory, setControl, releaseControl, putDashboardUi } from '../lib/api.js';
  import { warnings, refreshWarnings } from '../lib/warnings.js';
  import { daemonConfig, refreshConfig } from '../lib/config.js';
  import { sensorHistory } from '../lib/sensorHistory.js';
  import { virtualName, sensorLabel } from '../lib/sensors.js';
  import { tachSensorFor } from '../lib/tach.js';
  import { curvePaths, curveScale, evalCurve, sparklinePath, curvePoints, createAxisHold, clamp } from '../lib/curveMath.js';
  import { shortDevice, overview, trendArrow, temperatureUnit, chartCurve, spinPeriodSeconds, metricSensors, sensorKindFor } from '../lib/dashboard.js';
  import { openInGraph } from '../lib/page.js';
  import { isCombineType, nodeIdForCurveRef, deviceOf } from '../lib/graph/ids.js';
  import { sensorDisplay } from '../lib/graph/liveValues.js';
  import { axisMark } from '../lib/units.js';

  const CHART_W = 220;
  const CHART_H = 84;
  const CHART_PAD = 6;

  let inventory = $state(null);
  let editing = $state(false);
  let error = $state('');
  let hiddenIds = $derived(($daemonConfig && $daemonConfig.ui && $daemonConfig.ui.dashboard && $daemonConfig.ui.dashboard.hidden) || []);

  async function setHidden(id, hidden) {
    const next = hiddenIds.filter((entry) => entry !== id);
    if (hidden) next.push(id);
    try {
      await putDashboardUi({ hidden: next });
      await refreshConfig();
    } catch (err) {
      error = err.message;
    }
  }
  let pending = $state({});
  let drafts = $state({});

  onMount(async () => {
    try {
      inventory = await getInventory();
    } catch (err) {
      error = err.message;
    }
  });

  function fmtTemp(value) {
    return value === null || value === undefined ? '—' : value.toFixed(1);
  }

  function fmtInt(value) {
    return value === null || value === undefined ? '—' : String(Math.round(value));
  }

  let values = $derived(($snapshot && $snapshot.sensors) || {});
  let duties = $derived(($snapshot && $snapshot.duties) || {});
  let manual = $derived(($snapshot && $snapshot.manual) || {});
  let profile = $derived.by(() => {
    const cfg = $daemonConfig;
    if (!cfg) return null;
    const name = $snapshot ? $snapshot.active_profile : cfg.active_profile;
    return cfg.profiles[name] || null;
  });

  let allTemps = $derived.by(() => {
    if (!inventory) return [];
    const history = $sensorHistory;
    const hardware = inventory.sensors
      .filter((sensor) => sensor.kind === 'temp')
      .map((sensor) => ({ id: sensor.id, label: sensor.label, device: shortDevice(deviceOf(sensor.id)), unit: '°C', ranked: true }));
    const virtual = (inventory.virtual || []).map((entry) => ({
      id: entry.id,
      label: virtualName(entry.id),
      device: `virtual · ${entry.type}`,
      unit: temperatureUnit(entry.type),
      ranked: false,
    }));
    return [...hardware, ...virtual].map((sensor) => {
      const value = values[sensor.id] ?? null;
      const samples = history.get(sensor.id);
      const delta = history.delta(sensor.id);
      return {
        ...sensor,
        value,
        text: fmtTemp(value),
        hot: sensor.unit === '°C' && value !== null && value >= 60,
        spark: sparklinePath(samples, 120, 36),
        trend: samples.length > 1 ? trendArrow(delta) : '',
        trendText: samples.length > 1 ? `${delta > 0 ? '+' : ''}${delta.toFixed(1)}° over the last ${Math.round(samples.length * 5 / 60) || 1} min` : 'collecting history',
        hidden: hiddenIds.includes(sensor.id),
      };
    });
  });
  let temps = $derived(editing ? allTemps : allTemps.filter((sensor) => !sensor.hidden));

  let allMetrics = $derived.by(() => {
    if (!inventory) return [];
    const history = $sensorHistory;
    return metricSensors(inventory).map((metric) => {
      const value = values[metric.id] ?? null;
      const samples = history.get(metric.id);
      return {
        ...metric,
        value,
        spark: sparklinePath(samples, 120, 36),
        hidden: hiddenIds.includes(metric.id),
      };
    });
  });
  let metrics = $derived(editing ? allMetrics : allMetrics.filter((metric) => !metric.hidden));

  function curveFor(control) {
    if (!profile) return null;
    const curveId = profile.assignments ? profile.assignments[control.id] : undefined;
    if (!curveId) return null;
    const config = profile.curves ? profile.curves[curveId] : undefined;
    if (!config) return null;
    return { id: curveId, config };
  }

  const holdAxis = createAxisHold();

  function chartFor(curve, liveDuty) {
    const drawn = curve && profile ? chartCurve(curve.id, profile.curves, values) : null;
    const points = curvePoints(drawn ? drawn.config : null);
    if (!points) return null;
    const temp = values[drawn.config.sensor] ?? null;
    const kind = sensorKindFor(drawn.config.sensor, inventory);
    const axis = holdAxis(drawn.id, points, temp, kind);
    const paths = curvePaths(points, CHART_W, CHART_H, CHART_PAD, axis.max);
    const scale = curveScale(CHART_W, CHART_H, CHART_PAD, axis.max);
    const duty = liveDuty !== null ? liveDuty : temp === null ? null : evalCurve(points, temp);
    return {
      ...paths,
      showDot: temp !== null,
      dotX: temp === null ? 0 : scale.x(clamp(temp, 0, axis.max)),
      dotY: duty === null ? 0 : scale.y(duty),
      sensor: sensorLabel(drawn.config.sensor, inventory.sensors),
      via: drawn.via ? `${drawn.via} → ${drawn.id}` : '',
      temp,
      reading: sensorDisplay(temp, kind),
    };
  }

  function inputReadingText(value, kind) {
    const reading = sensorDisplay(value, kind);
    const mark = axisMark(reading.unit);
    return mark === '°' ? `${reading.text}°` : mark ? `${reading.text} ${mark}` : reading.text;
  }

  function curveInput(curve) {
    if (!curve) return '';
    const config = curve.config;
    if (isCombineType(config.type)) {
      return config.type === 'mix' ? (config.sources || []).join(' + ') : config.source || '';
    }
    return sensorLabel(config.sensor, inventory.sensors);
  }

  function curveKind(curve) {
    if (!curve) return '';
    const config = curve.config;
    return config.type === 'mix' ? `mix · ${config.mode}` : config.type;
  }

  let allFans = $derived.by(() => {
    if (!inventory) return [];
    return inventory.controls.map((control) => {
      const curve = curveFor(control);
      const tach = tachSensorFor(control.id, inventory.sensors);
      const rpm = tach ? values[tach.id] ?? null : null;
      const temp = curve && curve.config.sensor ? values[curve.config.sensor] ?? null : null;
      const inputKind = curve && curve.config.sensor ? sensorKindFor(curve.config.sensor, inventory) : undefined;
      const duty = duties[control.id] ?? null;
      return {
        id: control.id,
        label: control.label,
        device: shortDevice(deviceOf(control.id)),
        duty,
        manual: control.id in manual,
        rpm,
        rpmMissing: tach !== null && rpm === null,
        spinPeriod: spinPeriodSeconds(tach ? rpm : null, tach ? null : duty),
        isPump: /pump/i.test(control.label),
        curve,
        curveName: curve ? curve.id : '',
        curveKind: curveKind(curve),
        input: curveInput(curve),
        temp: temp === null ? '' : inputReadingText(temp, inputKind),
        chart: chartFor(curve, duty),
        hidden: hiddenIds.includes(control.id),
      };
    });
  });
  let fans = $derived(editing ? allFans : allFans.filter((fan) => !fan.hidden));
  let hiddenCount = $derived(
    allTemps.filter((sensor) => sensor.hidden).length +
    allFans.filter((fan) => fan.hidden).length +
    allMetrics.filter((metric) => metric.hidden).length
  );

  let stats = $derived(
    overview(
      temps.filter((sensor) => sensor.ranked).map((sensor) => sensor.value),
      fans.map((fan) => fan.duty),
      $warnings.length
    )
  );
  let manualCount = $derived(fans.filter((fan) => fan.manual).length);

  let draftValues = $derived.by(() => {
    const result = {};
    for (const fan of fans) {
      if (fan.manual && drafts[fan.id] !== undefined) result[fan.id] = drafts[fan.id];
      else result[fan.id] = fan.duty === null ? 50 : Math.round(fan.duty);
    }
    return result;
  });

  function setDraft(id, value) {
    drafts = { ...drafts, [id]: Number(value) };
  }

  async function hold(id) {
    if (pending[id]) return;
    pending = { ...pending, [id]: true };
    try {
      await setControl(id, draftValues[id]);
      await refreshWarnings();
    } catch (err) {
      error = err.message;
    } finally {
      pending = { ...pending, [id]: false };
    }
  }

  async function applyDraft(id, value) {
    setDraft(id, value);
    await hold(id);
  }

  async function release(id) {
    pending = { ...pending, [id]: true };
    try {
      await releaseControl(id);
      const next = { ...drafts };
      delete next[id];
      drafts = next;
      await refreshWarnings();
    } catch (err) {
      error = err.message;
    } finally {
      pending = { ...pending, [id]: false };
    }
  }

  function openCurve(fan) {
    if (!fan.curve) return;
    openInGraph({ nodeId: nodeIdForCurveRef(fan.curve.id, profile.curves) });
  }
</script>

<main class="page dashboard">
  <div class="overview">
    <div class="page-title">
      <span class="eyebrow">Overview</span>
      <h1>{stats.headline}</h1>
    </div>
    <button type="button" class="btn edit-toggle" class:primary={editing} data-testid="dashboard-edit" onclick={() => (editing = !editing)}>{editing ? 'Done' : hiddenCount > 0 ? `Edit · ${hiddenCount} hidden` : 'Edit'}</button>
    <div class="stats">
      <div class="stat"><span class="eyebrow">Hottest</span><span class="num mono temp">{fmtTemp(stats.maxTemp)}<small> °C</small></span></div>
      <div class="stat"><span class="eyebrow">Avg duty</span><span class="num mono duty">{fmtInt(stats.avgDuty)}<small> %</small></span></div>
      <div class="stat"><span class="eyebrow">Controls</span><span class="num mono">{fans.length}<small> · {manualCount} manual</small></span></div>
      <div class="stat"><span class="eyebrow">Warnings</span><span class="num mono" class:warn={$warnings.length > 0}>{$warnings.length}</span></div>
    </div>
  </div>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if !inventory}
    <p class="muted">Loading inventory.</p>
  {:else}
    <section>
      <div class="section-head"><h2 class="section-title">Temperatures</h2><span class="hint">last 10 minutes</span></div>
      <div class="temps">
        {#if temps.length === 0}<p class="muted">Every sensor is hidden. Use Edit to bring some back.</p>{/if}
        {#each temps as sensor (sensor.id)}
          <div class="card temp-card" class:dimmed={sensor.hidden} data-card-id={sensor.id}>
            <div class="temp-head"><span class="label">{sensor.label}</span><span class="device mono">{sensor.device}</span>{#if editing}<button type="button" class="btn hide-toggle" data-testid="card-hide" onclick={() => setHidden(sensor.id, !sensor.hidden)}>{sensor.hidden ? 'Show' : 'Hide'}</button>{/if}</div>
            <div class="temp-value">
              <span class="value mono" class:hot={sensor.hot}>{sensor.text}</span><span class="unit">{sensor.unit}</span>
              {#if sensor.trend}<span class="trend" title={sensor.trendText}>{sensor.trend}</span>{/if}
            </div>
            {#if sensor.spark}
              <svg viewBox="0 0 120 36" preserveAspectRatio="none" class="spark"><path d={sensor.spark} class:hot={sensor.hot} /></svg>
            {:else}
              <div class="no-spark mono">collecting history</div>
            {/if}
          </div>
        {/each}
      </div>
    </section>

    {#if metrics.length > 0}
      <section>
        <h2>Metrics</h2>
        <div class="grid metrics">
          {#each metrics as metric (metric.id)}
            {@const reading = sensorDisplay(metric.value, metric.kind)}
            <div class="card temp-card" class:dimmed={metric.hidden} data-card-id={metric.id}>
              <div class="temp-head"><span class="label">{metric.label}</span><span class="device mono">{metric.device}</span>{#if editing}<button type="button" class="btn hide-toggle" data-testid="card-hide" onclick={() => setHidden(metric.id, !metric.hidden)}>{metric.hidden ? 'Show' : 'Hide'}</button>{/if}</div>
              <div class="temp-value mono">{reading.text}<small>{reading.unit}</small></div>
              <svg class="spark" viewBox="0 0 120 36"><path d={metric.spark} /></svg>
            </div>
          {/each}
        </div>
      </section>
    {/if}

    <section>
      <div class="section-head"><h2 class="section-title">Fans</h2><span class="hint">where each control sits on its curve</span></div>
      <div class="fans">
        {#if fans.length === 0}<p class="muted">Every fan is hidden. Use Edit to bring some back.</p>{/if}
        {#each fans as fan (fan.id)}
          <div class="card control" class:dimmed={fan.hidden} data-card-id={fan.id}>
            <div class="control-head">
              <span class="spin-icon" class:spinning={fan.spinPeriod !== null} style:--spin-period="{fan.spinPeriod ?? 1}s" aria-hidden="true" data-testid="fan-icon" data-spinning={fan.spinPeriod !== null}>
                {#if fan.isPump}
                  <svg viewBox="0 0 24 24" width="22" height="22"><circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" stroke-width="1.6" /><path d="M12 12 L12 4.5 A7.5 7.5 0 0 1 18.5 8 Z M12 12 L18.5 16 A7.5 7.5 0 0 1 12 19.5 Z M12 12 L5.5 16 A7.5 7.5 0 0 1 5.5 8 Z" fill="currentColor" /><circle cx="12" cy="12" r="1.8" fill="var(--surface)" /></svg>
                {:else}
                  <svg viewBox="0 0 24 24" width="22" height="22"><path d="M12 12 C9 6 10 3 12 3 C14 3 15 6 12 12 Z M12 12 C18 9 21 10 21 12 C21 14 18 15 12 12 Z M12 12 C15 18 14 21 12 21 C10 21 9 18 12 12 Z M12 12 C6 15 3 14 3 12 C3 10 6 9 12 12 Z" fill="currentColor" /><circle cx="12" cy="12" r="2" fill="var(--surface)" /></svg>
                {/if}
              </span>
              <div class="names">
                <span class="label">{fan.label}</span><span class="device mono">{fan.device}</span>
              </div>
              {#if editing}<button type="button" class="btn hide-toggle" data-testid="card-hide" onclick={() => setHidden(fan.id, !fan.hidden)}>{fan.hidden ? 'Show' : 'Hide'}</button>{/if}
              {#if fan.manual}<span class="badge pill">manual</span>{/if}
              <div class="readings">
                <span class="duty-read mono">{fmtInt(fan.duty)}<small>%</small></span>
                <span class="rpm-read"><span class="mono" class:missing={fan.rpmMissing}>{fan.rpmMissing ? 'no tach' : fmtInt(fan.rpm)}</span><span class="eyebrow">rpm</span></span>
              </div>
            </div>
            {#if fan.chart}
              <div class="chart">
                <svg viewBox="0 0 {CHART_W} {CHART_H}" preserveAspectRatio="none">
                  <path d={fan.chart.area} class="area" />
                  <path d={fan.chart.line} class="line" vector-effect="non-scaling-stroke" />
                  {#if fan.chart.showDot}
                    <line x1={fan.chart.dotX} y1="0" x2={fan.chart.dotX} y2={CHART_H} class="live" vector-effect="non-scaling-stroke" />
                    <circle cx={fan.chart.dotX} cy={fan.chart.dotY} r="4" class="dot" />
                  {/if}
                </svg>
                <span class="chart-tag left mono">{fan.chart.sensor} {fan.chart.temp === null ? '' : `${fan.chart.reading.text}${fan.chart.reading.unit}`}</span>
                <span class="chart-tag right mono">{fan.chart.via || `${fan.curveName} · ${fan.curveKind}`}</span>
              </div>
            {:else}
              <div class="chart empty mono">
                {#if fan.curve}
                  <span class="duty-ink">{fan.curveName}</span><span>{fan.curveKind}</span>{#if fan.input}<span class="faint">← {fan.input}</span>{/if}
                {:else}
                  <span class="faint">no curve assigned</span>
                {/if}
              </div>
            {/if}
            <div class="control-body">
              {#if fan.manual}
                <input
                  type="range"
                  min="0"
                  max="100"
                  value={draftValues[fan.id]}
                  aria-label="{fan.label} duty"
                  disabled={pending[fan.id]}
                  oninput={(event) => setDraft(fan.id, event.target.value)}
                  onchange={(event) => applyDraft(fan.id, event.target.value)}
                />
                <span class="draft mono">{draftValues[fan.id]}%</span>
                <button type="button" class="btn release" disabled={pending[fan.id]} onclick={() => release(fan.id)}>Release</button>
              {:else}
                <button type="button" class="btn" data-testid="fan-manual" disabled={pending[fan.id]} onclick={() => hold(fan.id)}>Manual</button>
              {/if}
              <button type="button" class="btn" disabled={!fan.curve} onclick={() => openCurve(fan)}>Curve →</button>
            </div>
          </div>
        {/each}
      </div>
    </section>
  {/if}
</main>

<style>
  .edit-toggle {
    align-self: flex-end;
  }

  .hide-toggle {
    margin-left: auto;
    font-size: 11px;
    padding: 2px 8px;
  }

  .card.dimmed {
    opacity: 0.45;
  }

  .overview {
    display: flex;
    align-items: flex-end;
    gap: 24px;
    flex-wrap: wrap;
  }

  .stats {
    margin-left: auto;
    display: flex;
    gap: 28px;
    flex-wrap: wrap;
  }

  .stat {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .num {
    font-size: 20px;
    font-weight: var(--num-weight);
  }

  .num small {
    font-size: 12px;
    color: var(--muted);
  }

  .num.temp {
    color: var(--temp);
  }

  .num.duty {
    color: var(--duty);
  }

  .num.warn {
    color: var(--warn);
  }

  section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .temps,
  .grid.metrics {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 12px;
  }

  .temp-card {
    padding: 16px 16px 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 120px;
  }

  .card:hover {
    border-color: var(--line2);
  }

  .spin-icon {
    display: inline-flex;
    color: var(--muted);
    margin-right: 8px;
    flex: none;
  }

  .spin-icon svg {
    transform-origin: 50% 50%;
  }

  .spin-icon.spinning {
    color: var(--accent);
  }

  .spin-icon.spinning svg {
    animation: spin var(--spin-period, 1s) linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .spin-icon.spinning svg {
      animation: none;
    }
  }

  .temp-head,
  .control-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
  }

  .label {
    font-weight: 600;
    font-size: 13px;
  }

  .device {
    font-size: 10px;
    color: var(--faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .temp-value {
    display: flex;
    align-items: baseline;
    gap: 4px;
  }

  .value {
    font-size: 30px;
    font-weight: var(--num-weight);
    letter-spacing: -0.02em;
  }

  .value.hot {
    color: var(--temp);
  }

  .unit {
    font-size: 13px;
    color: var(--muted);
  }

  .trend {
    margin-left: auto;
    font-size: 12px;
    color: var(--muted);
  }

  .spark {
    width: 100%;
    height: 32px;
    display: block;
  }

  .spark path {
    fill: none;
    stroke: var(--ink);
    stroke-width: 1.5;
    vector-effect: non-scaling-stroke;
    stroke-linejoin: round;
  }

  .spark path.hot {
    stroke: var(--temp);
  }

  .no-spark {
    height: 32px;
    display: flex;
    align-items: center;
    font-size: 11px;
    color: var(--faint);
  }

  .fans {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 12px;
  }

  .control {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .control-head {
    align-items: center;
    gap: 10px;
  }

  .names {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .names .label {
    font-size: 14px;
  }

  .readings {
    margin-left: auto;
    display: flex;
    gap: 14px;
    align-items: baseline;
  }

  .duty-read {
    font-size: 24px;
    font-weight: var(--num-weight);
    color: var(--duty);
    letter-spacing: -0.02em;
  }

  .duty-read small {
    font-size: 12px;
    color: var(--muted);
  }

  .rpm-read {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    font-size: 16px;
  }

  .rpm-read .missing {
    color: var(--warn);
    font-size: 12px;
  }

  .rpm-read .eyebrow {
    font-size: 10px;
    color: var(--faint);
  }

  .chart {
    position: relative;
    background: var(--surface2);
    border-radius: var(--r);
    border: 1px solid var(--line);
    overflow: hidden;
    height: 84px;
  }

  .chart svg {
    width: 100%;
    height: 84px;
    display: block;
  }

  .chart .area {
    fill: var(--dutybg);
  }

  .chart .line {
    fill: none;
    stroke: var(--duty);
    stroke-width: 1.5;
    stroke-linejoin: round;
  }

  .chart .live {
    stroke: var(--temp);
    stroke-width: 1;
    stroke-dasharray: 3 3;
  }

  .chart .dot {
    fill: var(--temp);
    stroke: var(--surface2);
    stroke-width: 2;
  }

  .chart-tag {
    position: absolute;
    top: 6px;
    font-size: 10px;
    color: var(--muted);
  }

  .chart-tag.left {
    left: 8px;
    color: var(--temp);
  }

  .chart-tag.right {
    right: 8px;
  }

  .chart.empty {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    font-size: 11px;
    color: var(--muted);
  }

  .duty-ink {
    color: var(--duty);
  }

  .faint {
    color: var(--faint);
  }

  .control-body {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .control-body input[type='range'] {
    flex: 1;
    margin: 0;
    height: 4px;
    min-width: 0;
  }

  .draft {
    width: 3.2em;
    text-align: right;
    font-size: 12px;
    color: var(--muted);
  }

  @media (max-width: 720px) {
    .stats {
      margin-left: 0;
      display: grid;
      grid-template-columns: repeat(2, 1fr);
      gap: 0;
      border: 1px solid var(--line);
      border-radius: var(--r);
      width: 100%;
    }

    .stat {
      padding: 12px;
      border-right: 1px solid var(--line);
      border-bottom: 1px solid var(--line);
    }

    .stat:nth-child(2n) {
      border-right: none;
    }

    .stat:nth-last-child(-n + 2) {
      border-bottom: none;
    }

    .num {
      font-size: 18px;
    }

    .temps,
    .grid.metrics {
      grid-template-columns: repeat(2, 1fr);
      gap: 10px;
    }

    .temp-card {
      padding: 12px;
      min-height: 0;
    }

    .value {
      font-size: 26px;
    }

    .fans {
      grid-template-columns: 1fr;
    }

    .control-body {
      flex-wrap: wrap;
    }

    .control-body input[type='range'] {
      flex-basis: 100%;
    }
  }
</style>
