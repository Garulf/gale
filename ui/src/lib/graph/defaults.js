export function defaultCurve(type) {
  switch (type) {
    case 'point':
      return {
        type: 'point',
        points: [
          [30, 20],
          [70, 100],
        ],
        hysteresis: null,
        response: null,
      };
    case 'flat':
      return { type: 'flat', duty: 50 };
    case 'linear':
      return { type: 'linear', min_temp: 40, max_temp: 80, min_duty: 20, max_duty: 100, hysteresis: null, response: null };
    case 'mix':
      return { type: 'mix', sources: [], mode: 'max' };
    case 'sync':
      return { type: 'sync', source: '' };
    case 'offset':
      return { type: 'offset', source: '', add: 0, scale: 1 };
    case 'trigger':
      return { type: 'trigger', on_temp: 60, off_temp: 50, on_duty: 100, off_duty: 20, response: null };
    case 'target':
      return { type: 'target', target_temp: 60, step_pct_per_sec: 5, min_duty: 20, max_duty: 100, deadband: null, idle_temp: null };
    default:
      return { type: 'flat', duty: 50 };
  }
}
