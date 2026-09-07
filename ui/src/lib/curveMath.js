export function sortedPoints(points) {
  return [...points].sort((a, b) => a[0] - b[0]);
}

export function evalCurve(points, temp) {
  if (!points || points.length === 0) return null;
  const sorted = sortedPoints(points);
  if (temp <= sorted[0][0]) return sorted[0][1];
  const last = sorted[sorted.length - 1];
  if (temp >= last[0]) return last[1];
  for (let i = 1; i < sorted.length; i += 1) {
    if (temp <= sorted[i][0]) {
      const [t0, d0] = sorted[i - 1];
      const [t1, d1] = sorted[i];
      if (t1 === t0) return d1;
      return d0 + ((d1 - d0) * (temp - t0)) / (t1 - t0);
    }
  }
  return last[1];
}

const AXES = {
  temp: { max: 100, unit: '\u00b0C' },
  percent: { max: 100, unit: '%' },
  clock: { max: 4000, unit: 'MHz' },
  memory: { max: 32768, unit: 'MiB' },
  power: { max: 600, unit: 'W' },
  state: { max: 15, unit: '' },
};

const NICE_STEPS = [1, 2, 2.5, 5, 10];
const AXIS_HEADROOM = 1.1;
const TARGET_TICK_COUNT = 5;
const EPSILON = 1e-9;

function niceAtLeast(value) {
  const decade = 10 ** Math.floor(Math.log10(value));
  const step = NICE_STEPS.find((factor) => factor * decade >= value - EPSILON);
  return step === undefined ? 10 * decade : step * decade;
}

export function niceCeiling(value) {
  if (!Number.isFinite(value) || value <= 0) return 0;
  return niceAtLeast(value * AXIS_HEADROOM);
}

function largestObserved(observed) {
  if (!observed) return 0;
  return observed.reduce((best, value) => (Number.isFinite(value) && value > best ? value : best), 0);
}

export function axisFor(kind, observed = []) {
  const base = AXES[kind] || AXES.temp;
  const largest = largestObserved(observed);
  const max = largest > base.max ? niceCeiling(largest) : base.max;
  return { min: 0, max, unit: base.unit, step: niceAtLeast(max / TARGET_TICK_COUNT) };
}

function pointX(point) {
  if (Array.isArray(point)) return point[0];
  return point && typeof point === 'object' ? point.temp : point;
}

export function axisSpan(points, live, kind) {
  const observed = (points || []).map(pointX);
  observed.push(live);
  return axisFor(kind, observed);
}

export function axisTicks(axis) {
  const ticks = [];
  for (let value = axis.min; value < axis.max - EPSILON; value += axis.step) ticks.push(Math.round(value));
  const last = ticks[ticks.length - 1];
  if (last !== undefined && axis.max - last < axis.step / 2) ticks.pop();
  ticks.push(axis.max);
  return ticks;
}

export function pointFieldValue(field, value) {
  const numeric = Number(value);
  return field === 'duty' ? clamp(numeric, 0, 100) : numeric;
}

export function curveScale(width, height, pad = 0, xMax = 100) {
  return {
    x: (temp) => pad + (temp / xMax) * (width - 2 * pad),
    y: (duty) => pad + (height - 2 * pad) - (duty / 100) * (height - 2 * pad),
  };
}

export function curvePaths(points, width, height, pad = 0, xMax = 100) {
  if (!points || points.length === 0) return { line: '', area: '' };
  const sorted = sortedPoints(points);
  const { x, y } = curveScale(width, height, pad, xMax);
  const first = sorted[0];
  const last = sorted[sorted.length - 1];
  const segments = sorted.map(([t, d]) => `L ${x(t).toFixed(1)} ${y(d).toFixed(1)}`);
  const line = [`M ${x(0).toFixed(1)} ${y(first[1]).toFixed(1)}`, ...segments, `L ${x(xMax).toFixed(1)} ${y(last[1]).toFixed(1)}`].join(' ');
  const area = `${line} L ${x(xMax).toFixed(1)} ${y(0).toFixed(1)} L ${x(0).toFixed(1)} ${y(0).toFixed(1)} Z`;
  return { line, area };
}

export function sparklinePath(values, width, height) {
  if (!values || values.length < 2) return '';
  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = max - min || 1;
  return values
    .map((value, index) => {
      const x = (index / (values.length - 1)) * width;
      const y = height - 2 - ((value - min) / span) * (height - 4);
      return `${index === 0 ? 'M' : 'L'} ${x.toFixed(1)} ${y.toFixed(1)}`;
    })
    .join(' ');
}

export function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

export function nextPointTemp(temps, xMax = 100) {
  const sorted = [...temps].sort((a, b) => a - b);
  if (sorted.length === 0) return xMax / 2;
  const bounds = [0, ...sorted, xMax];
  let best = { width: -1, mid: null };
  for (let i = 1; i < bounds.length; i += 1) {
    const width = bounds[i] - bounds[i - 1];
    if (width > best.width) best = { width, mid: Math.round((bounds[i] + bounds[i - 1]) / 2) };
  }
  return sorted.includes(best.mid) ? null : best.mid;
}

export function curvePoints(config) {
  if (!config) return null;
  if (config.type === 'point') return config.points;
  if (config.type === 'linear') {
    return [
      [config.min_temp, config.min_duty],
      [config.max_temp, config.max_duty],
    ];
  }
  return null;
}
