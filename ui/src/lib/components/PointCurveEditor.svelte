<script>
  import { evalCurve, clamp, nextPointTemp } from '../curveMath.js';

  let { points, liveTemp = null, onChange, xMax = 100, xUnit = '°C' } = $props();

  const WIDTH = 560;
  const HEIGHT = 300;
  const PAD_LEFT = 44;
  const PAD_RIGHT = 16;
  const PAD_TOP = 16;
  const PAD_BOTTOM = 30;
  const PLOT_W = WIDTH - PAD_LEFT - PAD_RIGHT;
  const PLOT_H = HEIGHT - PAD_TOP - PAD_BOTTOM;

  let svgEl;
  let dragId = null;
  let didDrag = false;

  function xToPx(temp) {
    return PAD_LEFT + (temp / xMax) * PLOT_W;
  }

  function yToPx(duty) {
    return PAD_TOP + PLOT_H - (duty / 100) * PLOT_H;
  }

  function pxToTemp(px) {
    return clamp(((px - PAD_LEFT) / PLOT_W) * xMax, 0, xMax);
  }

  function pxToDuty(py) {
    return clamp(((PAD_TOP + PLOT_H - py) / PLOT_H) * 100, 0, 100);
  }

  function round1(value) {
    return Math.round(value * 10) / 10;
  }

  function sortedByTemp(list) {
    return [...list].sort((a, b) => a.temp - b.temp);
  }

  function clientToPlot(event) {
    const rect = svgEl.getBoundingClientRect();
    return {
      x: ((event.clientX - rect.left) * WIDTH) / rect.width,
      y: ((event.clientY - rect.top) * HEIGHT) / rect.height,
    };
  }

  function withPoint(id, updater) {
    onChange(points.map((point) => (point.id === id ? updater(point) : point)));
  }

  function pointerDown(id, event) {
    event.stopPropagation();
    dragId = id;
    svgEl.setPointerCapture(event.pointerId);
  }

  function pointerMove(event) {
    if (dragId === null) return;
    didDrag = true;
    const { x, y } = clientToPlot(event);
    const temp = round1(pxToTemp(x));
    const duty = round1(pxToDuty(y));
    withPoint(dragId, (point) => ({ ...point, temp, duty }));
  }

  function pointerUp(event) {
    if (dragId === null) return;
    dragId = null;
    if (svgEl.hasPointerCapture(event.pointerId)) svgEl.releasePointerCapture(event.pointerId);
  }

  function addPointAt(event) {
    if (didDrag) {
      didDrag = false;
      return;
    }
    if (event.target !== svgEl && !event.target.classList.contains('plot-bg')) return;
    const { x, y } = clientToPlot(event);
    onChange([...points, { id: crypto.randomUUID(), temp: round1(pxToTemp(x)), duty: round1(pxToDuty(y)) }]);
  }

  function addPoint() {
    const sorted = sortedByTemp(points);
    if (sorted.length === 0) {
      onChange([{ id: crypto.randomUUID(), temp: Math.round(xMax / 2), duty: 50 }]);
      return;
    }
    const temp = nextPointTemp(sorted.map((point) => point.temp), xMax);
    if (temp === null) return;
    const duty = Math.round(evalCurve(pairs(sorted), temp));
    onChange([...points, { id: crypto.randomUUID(), temp, duty }]);
  }

  function pairs(list) {
    return list.map((point) => [point.temp, point.duty]);
  }

  function removePoint(id) {
    if (points.length <= 2) return;
    onChange(points.filter((point) => point.id !== id));
  }

  function updateField(id, field, value) {
    const numeric = clamp(Number(value), 0, field === 'temp' ? xMax : 100);
    withPoint(id, (point) => ({ ...point, [field]: numeric }));
  }

  let sorted = $derived(sortedByTemp(points));
  let pathD = $derived(sorted.map((point, i) => `${i === 0 ? 'M' : 'L'} ${xToPx(point.temp)} ${yToPx(point.duty)}`).join(' '));
  let liveX = $derived(liveTemp === null || liveTemp === undefined ? null : xToPx(clamp(liveTemp, 0, xMax)));
  let liveY = $derived.by(() => {
    if (liveX === null || sorted.length === 0) return null;
    return yToPx(evalCurve(pairs(sorted), clamp(liveTemp, 0, xMax)));
  });

  let gridTemps = $derived([0, 0.2, 0.4, 0.6, 0.8, 1].map((f) => Math.round(f * xMax)));
  const gridDuties = [0, 25, 50, 75, 100];
</script>

<div class="editor">
  <div class="head"><span class="eyebrow">Curve</span><span class="tip">drag points · double-click to remove</span></div>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <svg
    bind:this={svgEl}
    viewBox="0 0 {WIDTH} {HEIGHT}"
    class="graph"
    role="img"
    aria-label="Point curve editor"
    onpointermove={pointerMove}
    onpointerup={pointerUp}
    onclick={addPointAt}
  >
    <rect class="plot-bg" x={PAD_LEFT} y={PAD_TOP} width={PLOT_W} height={PLOT_H} />
    {#each gridTemps as temp}
      <line x1={xToPx(temp)} y1={PAD_TOP} x2={xToPx(temp)} y2={PAD_TOP + PLOT_H} class="grid-line" />
      <text x={xToPx(temp)} y={HEIGHT - 10} class="axis-label" text-anchor="middle">{temp}{xUnit === '°C' ? '°' : ''}</text>
    {/each}
    {#each gridDuties as duty}
      <line x1={PAD_LEFT} y1={yToPx(duty)} x2={PAD_LEFT + PLOT_W} y2={yToPx(duty)} class="grid-line" />
      <text x={PAD_LEFT - 8} y={yToPx(duty)} class="axis-label" text-anchor="end" dominant-baseline="middle">{duty}</text>
    {/each}
    {#if liveX !== null}
      <line x1={liveX} y1={PAD_TOP} x2={liveX} y2={PAD_TOP + PLOT_H} class="live-marker" />
      {#if liveY !== null}
        <circle cx={liveX} cy={liveY} r="5" class="live-dot" />
      {/if}
    {/if}
    <path d={pathD} class="curve-line" />
    {#each sorted as point (point.id)}
      <circle
        cx={xToPx(point.temp)}
        cy={yToPx(point.duty)}
        r="7"
        class="curve-point"
        role="button"
        tabindex="0"
        aria-label="Curve point at {point.temp} {xUnit || 'input'}, {point.duty} percent"
        onpointerdown={(event) => pointerDown(point.id, event)}
        ondblclick={(event) => {
          event.stopPropagation();
          removePoint(point.id);
        }}
        onkeydown={(event) => {
          if (event.key === 'Delete' || event.key === 'Backspace') {
            event.stopPropagation();
            removePoint(point.id);
          }
        }}
      />
    {/each}
  </svg>

  <div class="points">
    {#each sorted as point (point.id)}
      <div class="point-row">
        <label class="field">{xUnit || 'x'}<input type="number" min="0" max={xMax} value={point.temp} oninput={(event) => updateField(point.id, 'temp', event.target.value)} /></label>
        <label class="field">%<input type="number" min="0" max="100" value={point.duty} oninput={(event) => updateField(point.id, 'duty', event.target.value)} /></label>
        <button type="button" class="btn remove" aria-label="Remove point" disabled={points.length <= 2} onclick={() => removePoint(point.id)}>×</button>
      </div>
    {/each}
    <button type="button" class="add" onclick={addPoint}>+ Add point</button>
  </div>
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }

  .tip {
    font-size: 11px;
    color: var(--faint);
  }

  .graph {
    width: 100%;
    aspect-ratio: 560 / 300;
    height: auto;
    display: block;
    background: var(--canvas);
    border: 1px solid var(--line);
    border-radius: var(--r);
    touch-action: none;
    cursor: crosshair;
  }

  .plot-bg {
    fill: transparent;
  }

  .grid-line {
    stroke: var(--grid);
    stroke-width: 1;
  }

  .axis-label {
    fill: var(--muted);
    font-size: 10px;
    font-family: var(--mono);
  }

  .curve-line {
    fill: none;
    stroke: var(--duty);
    stroke-width: 2;
    stroke-linejoin: round;
  }

  .curve-point {
    fill: var(--surface);
    stroke: var(--duty);
    stroke-width: 2;
    cursor: grab;
  }

  .curve-point:focus {
    outline: none;
    stroke: var(--accent);
  }

  .live-marker {
    stroke: var(--temp);
    stroke-width: 1;
    stroke-dasharray: 4 4;
  }

  .live-dot {
    fill: var(--temp);
    stroke: var(--canvas);
    stroke-width: 2;
  }

  .points {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .point-row {
    display: grid;
    grid-template-columns: 1fr 1fr auto;
    gap: 6px;
    align-items: center;
  }

  .remove {
    width: 28px;
    height: 28px;
    padding: 0;
    font-size: 14px;
  }

  .add {
    border: 1px dashed var(--line2);
    background: none;
    color: var(--muted);
    border-radius: var(--r);
    padding: 6px;
    font-size: 12px;
  }

  .add:hover {
    color: var(--ink);
  }

  @media (max-width: 720px) {
    .point-row .field {
      height: 44px;
    }

    .point-row .field input {
      font-size: 15px;
    }

    .remove {
      width: 44px;
      height: 44px;
    }

    .add {
      height: 44px;
    }
  }
</style>
