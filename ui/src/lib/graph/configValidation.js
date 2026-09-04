import { virtualSensorValidationError } from '../sensors.js';

function isBadNumber(value) {
  return value === null || value === undefined || typeof value !== 'number' || Number.isNaN(value);
}

const TRIGGER_FIELDS = ['on_temp', 'off_temp', 'on_duty', 'off_duty'];
const TARGET_FIELDS = ['target_temp', 'step_pct_per_sec', 'min_duty', 'max_duty'];

export function curveValidationError(curveId, curve) {
  const label = `Curve "${curveId}"`;
  if (curve.type === 'point') {
    for (const [temp, duty] of curve.points) {
      if (isBadNumber(temp)) return `${label}: a point's temperature must be a number`;
      if (isBadNumber(duty)) return `${label}: a point's duty must be a number`;
    }
    if (curve.hysteresis) {
      if (isBadNumber(curve.hysteresis.up)) return `${label}: hysteresis up must be a number`;
      if (isBadNumber(curve.hysteresis.down)) return `${label}: hysteresis down must be a number`;
    }
    if (curve.response) {
      if (isBadNumber(curve.response.rise_pct_per_sec)) return `${label}: response rise %/s must be a number`;
      if (isBadNumber(curve.response.fall_pct_per_sec)) return `${label}: response fall %/s must be a number`;
    }
  } else if (curve.type === 'flat') {
    if (isBadNumber(curve.duty)) return `${label}: duty must be a number`;
  } else if (curve.type === 'trigger') {
    for (const field of TRIGGER_FIELDS) {
      if (isBadNumber(curve[field])) return `${label}: ${field} must be a number`;
    }
  } else if (curve.type === 'target') {
    for (const field of TARGET_FIELDS) {
      if (isBadNumber(curve[field])) return `${label}: ${field} must be a number`;
    }
  }
  return '';
}

export function configValidationError(wireConfig) {
  for (const profileConfig of Object.values(wireConfig.profiles || {})) {
    for (const [curveId, curve] of Object.entries(profileConfig.curves || {})) {
      const invalid = curveValidationError(curveId, curve);
      if (invalid) return invalid;
    }
    for (const [name, sensor] of Object.entries(profileConfig.sensors || {})) {
      const invalid = virtualSensorValidationError(name, sensor);
      if (invalid) return invalid;
    }
  }
  return '';
}
