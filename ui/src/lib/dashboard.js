import { evalCurve, curvePoints } from './curveMath.js';

export function shortDevice(device) {
  if (device.startsWith('hwmon/')) return device.slice('hwmon/'.length);
  if (device.startsWith('corsair/commander-pro-')) return `cmd-pro ${device.slice('corsair/commander-pro-'.length)}`;
  if (device.startsWith('corsair/')) return device.slice('corsair/'.length);
  if (device.startsWith('nvidia/')) return `nvidia gpu ${device.slice('nvidia/'.length)}`;
  return device;
}

const SPIN_SLOWEST_SECONDS = 6;
const SPIN_FASTEST_SECONDS = 0.3;
const SPIN_VISUAL_RATIO = 20;
const DUTY_AT_SLOWEST_SECONDS = 40;

export function spinPeriodSeconds(rpm, duty = null) {
  if (rpm !== null && rpm !== undefined) {
    if (rpm <= 0) return null;
    const period = (60 / rpm) * SPIN_VISUAL_RATIO;
    return Number(Math.min(SPIN_SLOWEST_SECONDS, Math.max(SPIN_FASTEST_SECONDS, period)).toFixed(2));
  }
  if (duty === null || duty === undefined || duty <= 0) return null;
  const period = DUTY_AT_SLOWEST_SECONDS / duty;
  return Number(Math.min(SPIN_SLOWEST_SECONDS, Math.max(SPIN_FASTEST_SECONDS, period)).toFixed(2));
}

export function temperatureUnit(virtualType) {
  return virtualType === 'delta' ? '°C/min' : '°C';
}

export function headline({ maxTemp, avgDuty, warnings }) {
  if (warnings > 0) return warnings === 1 ? 'One warning needs attention' : `${warnings} warnings need attention`;
  if (maxTemp !== null && maxTemp >= 80) return 'System is running hot';
  if (avgDuty !== null && avgDuty >= 70) return 'Fans are working hard';
  return 'System is cool and quiet';
}

export function overview(temps, duties, warningCount) {
  const tempValues = temps.filter((value) => value !== null && value !== undefined);
  const dutyValues = duties.filter((value) => value !== null && value !== undefined);
  const maxTemp = tempValues.length ? Math.max(...tempValues) : null;
  const avgDuty = dutyValues.length ? dutyValues.reduce((sum, value) => sum + value, 0) / dutyValues.length : null;
  return { maxTemp, avgDuty, warnings: warningCount, headline: headline({ maxTemp, avgDuty, warnings: warningCount }) };
}

export function trendArrow(delta) {
  if (delta > 0.5) return '↗';
  if (delta < -0.5) return '↘';
  return '→';
}


function liveDuty(config, values) {
  const points = curvePoints(config);
  const temp = values[config.sensor];
  if (!points || temp === null || temp === undefined) return null;
  return evalCurve(points, temp);
}

export function chartCurve(curveId, curves, values, depth = 0) {
  const config = curves ? curves[curveId] : undefined;
  if (!config || depth > 8) return null;
  if (config.type === 'point' || config.type === 'linear') return { id: curveId, config, via: '' };
  if (config.type === 'trigger') {
    const points = [
      [config.off_temp, config.off_duty],
      [config.on_temp, config.on_duty],
    ];
    return { id: curveId, config: { type: 'point', sensor: config.sensor, points }, via: '' };
  }
  if (config.type === 'sync' || config.type === 'offset') {
    const inner = chartCurve(config.source, curves, values, depth + 1);
    return inner && { ...inner, via: curveId };
  }
  if (config.type === 'mix') {
    const candidates = (config.sources || [])
      .map((source) => chartCurve(source, curves, values, depth + 1))
      .filter(Boolean)
      .map((candidate) => ({ candidate, duty: liveDuty(candidate.config, values) }));
    if (candidates.length === 0) return null;
    const known = candidates.filter((entry) => entry.duty !== null);
    const pool = known.length > 0 ? known : candidates;
    const pick = config.mode === 'min'
      ? pool.reduce((best, entry) => (entry.duty < best.duty ? entry : best))
      : config.mode === 'max'
        ? pool.reduce((best, entry) => (entry.duty > best.duty ? entry : best))
        : pool[0];
    return { ...pick.candidate, via: curveId };
  }
  return null;
}
