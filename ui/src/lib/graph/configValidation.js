import { virtualSensorValidationError, VIRTUAL_PREFIX, virtualName } from '../sensors.js';

function isBadNumber(value) {
  return value === null || value === undefined || typeof value !== 'number' || Number.isNaN(value);
}

const LINEAR_FIELDS = ['min_temp', 'max_temp', 'min_duty', 'max_duty'];
const TRIGGER_FIELDS = ['on_temp', 'off_temp', 'on_duty', 'off_duty'];
const TARGET_FIELDS = ['target_temp', 'step_pct_per_sec', 'min_duty', 'max_duty'];

function smoothingError(label, curve) {
  if (curve.hysteresis) {
    if (isBadNumber(curve.hysteresis.up)) return `${label}: hysteresis up must be a number`;
    if (isBadNumber(curve.hysteresis.down)) return `${label}: hysteresis down must be a number`;
  }
  if (curve.response) {
    if (isBadNumber(curve.response.rise_pct_per_sec)) return `${label}: response rise %/s must be a number`;
    if (isBadNumber(curve.response.fall_pct_per_sec)) return `${label}: response fall %/s must be a number`;
  }
  return '';
}

export function curveValidationError(curveId, curve) {
  const label = `Curve "${curveId}"`;
  if (curve.type === 'point') {
    for (const [temp, duty] of curve.points) {
      if (isBadNumber(temp)) return `${label}: a point's temperature must be a number`;
      if (isBadNumber(duty)) return `${label}: a point's duty must be a number`;
    }
    return smoothingError(label, curve);
  } else if (curve.type === 'flat') {
    if (isBadNumber(curve.duty)) return `${label}: duty must be a number`;
  } else if (curve.type === 'linear') {
    for (const field of LINEAR_FIELDS) {
      if (isBadNumber(curve[field])) return `${label}: ${field} must be a number`;
    }
    if (curve.max_temp < curve.min_temp) return `${label}: max_temp must not be below min_temp`;
    return smoothingError(label, curve);
  } else if (curve.type === 'trigger') {
    for (const field of TRIGGER_FIELDS) {
      if (isBadNumber(curve[field])) return `${label}: ${field} must be a number`;
    }
    return smoothingError(label, curve);
  } else if (curve.type === 'target') {
    for (const field of TARGET_FIELDS) {
      if (isBadNumber(curve[field])) return `${label}: ${field} must be a number`;
    }
    if (curve.deadband != null && (isBadNumber(curve.deadband) || curve.deadband < 0)) return `${label}: deadband must be zero or more`;
    if (curve.idle_temp != null && isBadNumber(curve.idle_temp)) return `${label}: idle_temp must be a number`;
  }
  return '';
}

function sensorInputs(sensor) {
  if (Array.isArray(sensor.inputs)) return sensor.inputs;
  return sensor.input !== undefined ? [sensor.input] : [];
}

function virtualDependencies(sensors, name) {
  return sensorInputs(sensors[name] || {})
    .filter((input) => typeof input === 'string' && input.startsWith(VIRTUAL_PREFIX))
    .map(virtualName);
}

export function virtualSensorCycleError(sensors) {
  const visited = new Set();

  function visit(startName) {
    if (visited.has(startName)) return '';
    const stack = [{ name: startName, children: virtualDependencies(sensors, startName), next: 0 }];
    while (stack.length > 0) {
      const top = stack[stack.length - 1];
      if (top.next < top.children.length) {
        const child = top.children[top.next];
        top.next += 1;
        if (visited.has(child)) continue;
        const pos = stack.findIndex((frame) => frame.name === child);
        if (pos !== -1) {
          const cycle = stack.slice(pos).map((frame) => frame.name);
          cycle.push(child);
          return `virtual sensor cycle: ${cycle.join(' -> ')}`;
        }
        stack.push({ name: child, children: virtualDependencies(sensors, child), next: 0 });
      } else {
        const done = stack.pop();
        visited.add(done.name);
      }
    }
    return '';
  }

  for (const name of Object.keys(sensors).sort()) {
    const error = visit(name);
    if (error) return error;
  }
  return '';
}

export function undefinedVirtualReferenceError(curves, sensors) {
  for (const [name, sensor] of Object.entries(sensors)) {
    for (const input of sensorInputs(sensor)) {
      if (typeof input === 'string' && input.startsWith(VIRTUAL_PREFIX) && !(virtualName(input) in sensors)) {
        return `virtual sensor '${name}' references undefined virtual sensor '${input}'`;
      }
    }
  }
  for (const [curveId, curve] of Object.entries(curves)) {
    const sensorRef = curve.sensor;
    if (
      typeof sensorRef === 'string' &&
      sensorRef.startsWith(VIRTUAL_PREFIX) &&
      !(virtualName(sensorRef) in sensors)
    ) {
      return `curve '${curveId}' references undefined virtual sensor '${sensorRef}'`;
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
    const cycleError = virtualSensorCycleError(profileConfig.sensors || {});
    if (cycleError) return cycleError;
    const referenceError = undefinedVirtualReferenceError(
      profileConfig.curves || {},
      profileConfig.sensors || {}
    );
    if (referenceError) return referenceError;
  }
  return '';
}
