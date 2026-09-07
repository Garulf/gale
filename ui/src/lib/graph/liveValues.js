import { KIND_UNITS } from '../units.js';
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

export function curveOutput(snap, curveId) {
  if (!snap || !snap.curves) return null;
  const value = snap.curves[curveId];
  return value === null || value === undefined ? null : value;
}

export function formatDeltaRate(value) {
  return value === null || value === undefined ? 'n/a' : `${value.toFixed(1)} C/min`;
}

export function isOverridden(snap, targetHandle) {
  if (!snap || !snap.overrides) return false;
  return snap.overrides.includes(targetHandle);
}

export { KIND_UNITS };

export function isCurveInputKind(kind) {
  return kind === undefined || (kind !== 'rpm' && kind !== 'duty');
}

export function sensorDisplay(value, kind) {
  const resolved = kind in KIND_UNITS ? kind : 'temp';
  const missing = value === null || value === undefined;
  let text = '—';
  if (!missing) {
    if (resolved === 'temp' || resolved === 'power') text = value.toFixed(1);
    else if (resolved === 'state') text = `P${Math.round(value)}`;
    else text = String(Math.round(value));
  }
  return { text, unit: KIND_UNITS[resolved] };
}
