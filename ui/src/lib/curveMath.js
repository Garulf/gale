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

export function curveScale(width, height, pad = 0) {
  return {
    x: (temp) => pad + (temp / 100) * (width - 2 * pad),
    y: (duty) => pad + (height - 2 * pad) - (duty / 100) * (height - 2 * pad),
  };
}

export function curvePaths(points, width, height, pad = 0) {
  if (!points || points.length === 0) return { line: '', area: '' };
  const sorted = sortedPoints(points);
  const { x, y } = curveScale(width, height, pad);
  const first = sorted[0];
  const last = sorted[sorted.length - 1];
  const segments = sorted.map(([t, d]) => `L ${x(t).toFixed(1)} ${y(d).toFixed(1)}`);
  const line = [`M ${x(0).toFixed(1)} ${y(first[1]).toFixed(1)}`, ...segments, `L ${x(100).toFixed(1)} ${y(last[1]).toFixed(1)}`].join(' ');
  const area = `${line} L ${x(100).toFixed(1)} ${y(0).toFixed(1)} L ${x(0).toFixed(1)} ${y(0).toFixed(1)} Z`;
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
