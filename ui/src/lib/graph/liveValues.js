import { nodeKind, nodeName } from './ids.js';

export function tempValue(snap, sourceNodeId, sourceHandle) {
  if (!snap || !snap.sensors) return null;
  const key = nodeKind(sourceNodeId) === 'virtual' ? `virtual/${nodeName(sourceNodeId)}` : sourceHandle;
  const value = snap.sensors[key];
  return value === null || value === undefined ? null : value;
}

export function dutyValue(snap, targetHandle) {
  if (!snap || !snap.duties) return null;
  const value = snap.duties[targetHandle];
  return value === null || value === undefined ? null : value;
}

export function formatTemp(value) {
  return value === null || value === undefined ? 'n/a' : `${value.toFixed(1)} C`;
}

export function formatDuty(value) {
  return value === null || value === undefined ? 'n/a' : `${Math.round(value)}%`;
}

export function formatRpm(value) {
  return value === null || value === undefined ? 'n/a' : `${Math.round(value)} RPM`;
}

export function formatSensor(value, kind) {
  if (kind === 'rpm') return formatRpm(value);
  if (kind === 'duty') return formatDuty(value);
  return formatTemp(value);
}
