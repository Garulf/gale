export const VIRTUAL_PREFIX = 'virtual/';
const MULTI_INPUT_TYPES = ['max', 'min', 'mean', 'sum', 'subtract'];
export const SENSOR_TYPES = ['max', 'min', 'mean', 'sum', 'subtract', 'offset', 'delta', 'webhook'];

export function virtualId(name) {
  return `${VIRTUAL_PREFIX}${name}`;
}

export function virtualName(id) {
  return id.slice(VIRTUAL_PREFIX.length);
}

export function isVirtualId(id) {
  return typeof id === 'string' && id.startsWith(VIRTUAL_PREFIX);
}

export function sensorLabel(id, hardwareSensors) {
  if (!id) return '';
  if (isVirtualId(id)) return virtualName(id);
  const sensor = (hardwareSensors || []).find((entry) => entry.id === id);
  return sensor ? sensor.label : id;
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
    case 'sum':
      return { type: 'sum', inputs: [] };
    case 'subtract':
      return { type: 'subtract', inputs: [] };
    case 'offset':
      return { type: 'offset', input: '', add: 0, scale: 1 };
    case 'delta':
      return { type: 'delta', input: '', window_s: 30 };
    case 'webhook':
      return { type: 'webhook', token: '', timeout_s: null };
    default:
      return { type: 'max', inputs: [] };
  }
}

export function virtualSensorInputs(sensor) {
  if (Array.isArray(sensor.inputs)) return sensor.inputs;
  if (sensor.input !== undefined) return [sensor.input];
  return [];
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
  if (MULTI_INPUT_TYPES.includes(sensor.type)) {
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
  } else if (sensor.type === 'webhook') {
    if (sensor.timeout_s != null && (isBadNumber(sensor.timeout_s) || sensor.timeout_s <= 0)) {
      return `${label}: timeout_s must be a positive number`;
    }
  }
  return '';
}
