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

export function formatDeltaRate(value) {
  return value === null || value === undefined ? 'n/a' : `${value.toFixed(1)} C/min`;
}

export function isOverridden(snap, targetHandle) {
  if (!snap || !snap.overrides) return false;
  return snap.overrides.includes(targetHandle);
}

const UNITS = { rpm: 'rpm', duty: '%', temp: '°C' };

export function sensorDisplay(value, kind) {
  const resolved = kind in UNITS ? kind : 'temp';
  const missing = value === null || value === undefined;
  const text = missing ? '—' : resolved === 'temp' ? value.toFixed(1) : String(Math.round(value));
  return { text, unit: UNITS[resolved] };
}
