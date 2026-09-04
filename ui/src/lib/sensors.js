export const VIRTUAL_PREFIX = 'virtual/';
export const SENSOR_TYPES = ['max', 'min', 'mean', 'offset', 'delta'];

export function virtualId(name) {
  return `${VIRTUAL_PREFIX}${name}`;
}

export function virtualName(id) {
  return id.slice(VIRTUAL_PREFIX.length);
}

export function isVirtualId(id) {
  return typeof id === 'string' && id.startsWith(VIRTUAL_PREFIX);
}

export function sensorOptions(hardwareSensors, virtualNames, excludeId) {
  const hardware = (hardwareSensors || []).map((sensor) => ({
    id: sensor.id,
    label: sensor.label,
  }));
  const virtual = [...virtualNames]
    .sort()
    .map((name) => ({ id: virtualId(name), label: virtualId(name) }));
  return [...hardware, ...virtual].filter((option) => option.id !== excludeId);
}

export function defaultVirtualSensor(type) {
  switch (type) {
    case 'max':
      return { type: 'max', inputs: [] };
    case 'min':
      return { type: 'min', inputs: [] };
    case 'mean':
      return { type: 'mean', inputs: [], window_s: null };
    case 'offset':
      return { type: 'offset', input: '', add: 0, scale: 1 };
    case 'delta':
      return { type: 'delta', input: '', window_s: 30 };
    default:
      return { type: 'max', inputs: [] };
  }
}

export function virtualSensorInputs(sensor) {
  return sensor.inputs || [sensor.input];
}

export function virtualSensorReferences(name, curves, sensors) {
  const refs = [];
  const id = virtualId(name);
  for (const [curveId, curve] of Object.entries(curves)) {
    if (curve.sensor === id) refs.push(`curve "${curveId}"`);
  }
  for (const [otherName, sensor] of Object.entries(sensors)) {
    if (otherName === name) continue;
    if (virtualSensorInputs(sensor).includes(id)) refs.push(`sensor "${otherName}"`);
  }
  return refs;
}

function isBadNumber(value) {
  return value === null || value === undefined || typeof value !== 'number' || Number.isNaN(value);
}

export function virtualSensorValidationError(name, sensor) {
  const label = `Sensor "${name}"`;
  if (!name) return `${label}: name must not be empty`;
  if (name.includes('/')) return `${label}: name must not contain "/"`;
  if (sensor.type === 'max' || sensor.type === 'min' || sensor.type === 'mean') {
    if (sensor.inputs.length === 0) return `${label}: at least one input is required`;
    if (sensor.type === 'mean' && sensor.window_s != null) {
      if (isBadNumber(sensor.window_s) || sensor.window_s <= 0) {
        return `${label}: window_s must be a positive number`;
      }
    }
  } else if (sensor.type === 'offset') {
    if (!sensor.input) return `${label}: input is required`;
    if (isBadNumber(sensor.add)) return `${label}: add must be a number`;
    if (isBadNumber(sensor.scale)) return `${label}: scale must be a number`;
  } else if (sensor.type === 'delta') {
    if (!sensor.input) return `${label}: input is required`;
    if (isBadNumber(sensor.window_s) || sensor.window_s <= 0) {
      return `${label}: window_s must be a positive number`;
    }
  }
  return '';
}
