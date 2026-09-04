<script>
  let { points, liveTemp = null, onChange } = $props();

  const WIDTH = 560;
  const HEIGHT = 280;
  const PAD_LEFT = 40;
  const PAD_BOTTOM = 28;
  const PAD_TOP = 12;
  const PAD_RIGHT = 12;
  const PLOT_W = WIDTH - PAD_LEFT - PAD_RIGHT;
  const PLOT_H = HEIGHT - PAD_TOP - PAD_BOTTOM;

  let svgEl;
  let dragId = null;
  let didDrag = false;

  function xToPx(temp) {
    return PAD_LEFT + (temp / 100) * PLOT_W;
  }

  function yToPx(duty) {
    return PAD_TOP + PLOT_H - (duty / 100) * PLOT_H;
  }

  function pxToTemp(px) {
    return clamp(((px - PAD_LEFT) / PLOT_W) * 100, 0, 100);
  }

  function pxToDuty(py) {
    return clamp(((PAD_TOP + PLOT_H - py) / PLOT_H) * 100, 0, 100);
  }

  function clamp(value, min, max) {
    return Math.min(max, Math.max(min, value));
  }

  function round1(value) {
    return Math.round(value * 10) / 10;
  }

  function sortedByTemp(list) {
    return [...list].sort((a, b) => a.temp - b.temp);
  }

  function clientToPlot(event) {
    const rect = svgEl.getBoundingClientRect();
    const scaleX = WIDTH / rect.width;
    const scaleY = HEIGHT / rect.height;
    return {
      x: (event.clientX - rect.left) * scaleX,
      y: (event.clientY - rect.top) * scaleY,
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

  function releasePointerCapture(event) {
    if (!svgEl.hasPointerCapture(event.pointerId)) return;
    svgEl.releasePointerCapture(event.pointerId);
  }

  function pointerUp(event) {
    if (dragId === null) return;
    dragId = null;
    releasePointerCapture(event);
  }

  function addPoint(event) {
    if (didDrag) {
      didDrag = false;
      return;
    }
    if (event.target !== svgEl && !event.target.classList.contains('plot-bg')) return;
    const { x, y } = clientToPlot(event);
    const point = {
      id: crypto.randomUUID(),
      temp: round1(pxToTemp(x)),
      duty: round1(pxToDuty(y)),
    };
    onChange([...points, point]);
  }

  function removePoint(id) {
    if (points.length <= 2) return;
    onChange(points.filter((point) => point.id !== id));
  }

  function updateField(id, field, value) {
    const numeric = clamp(Number(value), 0, 100);
    withPoint(id, (point) => ({ ...point, [field]: numeric }));
  }

  let pathD = $derived(
    points.length
      ? sortedByTemp(points)
          .map((point, i) => `${i === 0 ? 'M' : 'L'} ${xToPx(point.temp)} ${yToPx(point.duty)}`)
          .join(' ')
      : ''
  );

  let gridTemps = [0, 20, 40, 60, 80, 100];
  let gridDuties = [0, 20, 40, 60, 80, 100];
</script>

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
  onclick={addPoint}
>
  <rect
    class="plot-bg"
    x={PAD_LEFT}
    y={PAD_TOP}
    width={PLOT_W}
    height={PLOT_H}
    fill="#14161a"
  />

  {#each gridTemps as temp}
    <line
      x1={xToPx(temp)}
      y1={PAD_TOP}
      x2={xToPx(temp)}
      y2={PAD_TOP + PLOT_H}
      class="grid-line"
    />
    <text x={xToPx(temp)} y={HEIGHT - 8} class="axis-label" text-anchor="middle">{temp}</text>
  {/each}

  {#each gridDuties as duty}
    <line
      x1={PAD_LEFT}
      y1={yToPx(duty)}
      x2={PAD_LEFT + PLOT_W}
      y2={yToPx(duty)}
      class="grid-line"
    />
    <text x={PAD_LEFT - 8} y={yToPx(duty) + 4} class="axis-label" text-anchor="end">{duty}</text>
  {/each}

  <text x={PAD_LEFT + PLOT_W / 2} y={HEIGHT - 2} class="axis-title" text-anchor="middle">
    temperature (°C)
  </text>
  <text
    x={12}
    y={PAD_TOP + PLOT_H / 2}
    class="axis-title"
    text-anchor="middle"
    transform="rotate(-90 12 {PAD_TOP + PLOT_H / 2})"
  >
    duty (%)
  </text>

  {#if liveTemp !== null && liveTemp !== undefined}
    <line
      x1={xToPx(clamp(liveTemp, 0, 100))}
      y1={PAD_TOP}
      x2={xToPx(clamp(liveTemp, 0, 100))}
      y2={PAD_TOP + PLOT_H}
      class="live-marker"
    />
  {/if}

  <path d={pathD} class="curve-line" />

  {#each sortedByTemp(points) as point (point.id)}
    <circle
      cx={xToPx(point.temp)}
      cy={yToPx(point.duty)}
      r="6"
      class="curve-point"
      role="button"
      tabindex="0"
      aria-label="Curve point at {point.temp} degrees, {point.duty} percent"
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

<table class="points-table">
  <thead>
    <tr>
      <th>Temp (°C)</th>
      <th>Duty (%)</th>
      <th></th>
    </tr>
  </thead>
  <tbody>
    {#each sortedByTemp(points) as point (point.id)}
      <tr>
        <td>
          <input
            type="number"
            min="0"
            max="100"
            value={point.temp}
            oninput={(event) => updateField(point.id, 'temp', event.target.value)}
          />
        </td>
        <td>
          <input
            type="number"
            min="0"
            max="100"
            value={point.duty}
            oninput={(event) => updateField(point.id, 'duty', event.target.value)}
          />
        </td>
        <td>
          <button
            class="remove"
            disabled={points.length <= 2}
            onclick={() => removePoint(point.id)}
          >
            Remove
          </button>
        </td>
      </tr>
    {/each}
  </tbody>
</table>
<p class="hint">Click empty space to add a point. Double-click a point to remove it.</p>

<style>
  .graph {
    width: 100%;
    max-width: 560px;
    background: #1b1e24;
    border: 1px solid #2a2f38;
    border-radius: 8px;
    touch-action: none;
    cursor: crosshair;
  }

  .grid-line {
    stroke: #2a2f38;
    stroke-width: 1;
  }

  .axis-label {
    fill: #9ca3af;
    font-size: 10px;
  }

  .axis-title {
    fill: #9ca3af;
    font-size: 11px;
  }

  .curve-line {
    fill: none;
    stroke: #38bdf8;
    stroke-width: 2;
  }

  .curve-point {
    fill: #38bdf8;
    stroke: #14161a;
    stroke-width: 1.5;
    cursor: grab;
  }

  .live-marker {
    stroke: #f59e0b;
    stroke-width: 1.5;
    stroke-dasharray: 4 3;
  }

  .points-table {
    margin-top: 0.75rem;
    border-collapse: collapse;
    font-size: 0.85rem;
  }

  .points-table th {
    text-align: left;
    opacity: 0.7;
    font-weight: normal;
    padding: 0.2rem 0.5rem;
  }

  .points-table td {
    padding: 0.2rem 0.5rem;
  }

  .points-table input {
    width: 4.5rem;
    background: #14161a;
    color: inherit;
    border: 1px solid #2a2f38;
    border-radius: 4px;
    padding: 0.2rem;
  }

  button.remove {
    background: none;
    border: 1px solid #2a2f38;
    color: inherit;
    border-radius: 4px;
    padding: 0.15rem 0.5rem;
    cursor: pointer;
  }

  button.remove:disabled {
    opacity: 0.35;
    cursor: default;
  }

  .hint {
    opacity: 0.55;
    font-size: 0.8rem;
    margin: 0.4rem 0 0;
  }
</style>
