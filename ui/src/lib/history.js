export const HISTORY_WINDOW_MS = 10 * 60 * 1000;
export const HISTORY_STEP_MS = 5000;

export function createHistory(windowMs = HISTORY_WINDOW_MS, stepMs = HISTORY_STEP_MS) {
  const series = new Map();

  function record(values, now) {
    for (const [id, value] of Object.entries(values || {})) {
      if (value === null || value === undefined) continue;
      let entry = series.get(id);
      if (!entry) {
        entry = [];
        series.set(id, entry);
      }
      const last = entry[entry.length - 1];
      if (last && now - last.t < stepMs) continue;
      entry.push({ t: now, v: value });
      const cutoff = now - windowMs;
      while (entry.length && entry[0].t < cutoff) entry.shift();
    }
  }

  function get(id) {
    return (series.get(id) || []).map((sample) => sample.v);
  }

  function delta(id) {
    const entry = series.get(id);
    if (!entry || entry.length < 2) return 0;
    return entry[entry.length - 1].v - entry[0].v;
  }

  return { record, get, delta };
}
