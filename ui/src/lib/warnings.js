import { writable, get } from 'svelte/store';
import { getWarnings } from './api.js';
import { nodeIdForCurveRef } from './graph/ids.js';
import { virtualSensorInputs } from './sensors.js';

export const warnings = writable([]);
export const dismissedWarnings = writable([]);

export function pruneDismissed(dismissed, current) {
  return dismissed.filter((warning) => current.includes(warning));
}

export function dismissWarning(warning) {
  dismissedWarnings.update((dismissed) => [...dismissed, warning]);
}

export async function refreshWarnings() {
  let current = [];
  try {
    const result = await getWarnings();
    current = (result && result.warnings) || [];
  } catch (error) {
    current = [];
  }
  warnings.set(current);
  dismissedWarnings.set(pruneDismissed(get(dismissedWarnings), current));
}

const FAIL_SAFE_PCT = 100;
const MISSING_SENSOR = /^referenced sensor not found on hardware: (.+)$/;
const UNWIRED_CURVE = /^curve '([^']+)' has no input wired/;
const NON_INPUT_KIND = /^curve '([^']+)' reads '([^']+)', a (\S+) sensor/;
const VIRTUAL_SENSOR = /^virtual sensor '([^']+)'/;

function listText(names) {
  if (names.length <= 1) return names.join('');
  return `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`;
}

function curveNodeRef(curveId, profile) {
  return nodeIdForCurveRef(curveId, profile.curves || {});
}

function curvesReading(profile, sensorId) {
  return Object.entries(profile.curves || {})
    .filter(([, curve]) => curve.sensor === sensorId)
    .map(([id]) => id);
}

function virtualsReading(profile, sensorId) {
  return Object.entries(profile.sensors || {})
    .filter(([, sensor]) => virtualSensorInputs(sensor).includes(sensorId))
    .map(([name]) => name);
}

function controlsDrivenBy(profile, curveIds) {
  return Object.entries(profile.assignments || {})
    .filter(([, curveId]) => curveIds.includes(curveId))
    .map(([controlId]) => controlId);
}

function controlName(controlId, labels) {
  return (labels && labels[controlId]) || controlId.split('/').pop();
}

function runningAt(controls, labels, duties) {
  const known = controls.map((id) => (duties ? duties[id] : undefined)).filter((value) => typeof value === 'number');
  const pct = known.length > 0 && known.every((value) => value === known[0]) ? Math.round(known[0]) : FAIL_SAFE_PCT;
  const names = controls.map((id) => controlName(id, labels));
  return `${listText(names)} ${names.length === 1 ? 'is' : 'are'} running at ${pct}%`;
}

function plain(message) {
  return { message, headline: message, detail: '', raw: '', nodeId: null, curves: [] };
}

function describeMissingSensor(message, sensorId, profile, labels, duties) {
  const curves = curvesReading(profile, sensorId);
  const controls = controlsDrivenBy(profile, curves);
  const virtuals = virtualsReading(profile, sensorId);
  const base = { message, raw: sensorId, curves };
  if (curves.length > 0) {
    const reads = curves.length === 1 ? 'reads' : 'read';
    return {
      ...base,
      headline: controls.length > 0 ? runningAt(controls, labels, duties) : `${listText(curves)} ${curves.length === 1 ? 'has' : 'have'} no input`,
      detail: `${listText(curves)} ${reads} a sensor that is not on this machine.`,
      nodeId: curveNodeRef(curves[0], profile),
    };
  }
  if (virtuals.length > 0) {
    return {
      ...base,
      headline: `${listText(virtuals)} ${virtuals.length === 1 ? 'is' : 'are'} missing an input`,
      detail: 'One of its inputs is not on this machine.',
      nodeId: `virtual:${virtuals[0]}`,
    };
  }
  return { ...base, headline: 'A referenced sensor is not on this machine', detail: '', nodeId: null };
}

export function describeWarning(message, { profile = null, labels = null, duties = null } = {}) {
  if (!profile) return plain(message);
  let match = MISSING_SENSOR.exec(message);
  if (match) return describeMissingSensor(message, match[1].trim(), profile, labels, duties);
  match = UNWIRED_CURVE.exec(message);
  if (match) {
    const curveId = match[1];
    const controls = controlsDrivenBy(profile, [curveId]);
    return {
      message,
      headline: controls.length > 0 ? runningAt(controls, labels, duties) : `${curveId} has no input`,
      detail: `${curveId} has no input wired, so it fails safe to ${FAIL_SAFE_PCT}%.`,
      raw: curveId,
      nodeId: curveNodeRef(curveId, profile),
      curves: [curveId],
    };
  }
  match = NON_INPUT_KIND.exec(message);
  if (match) {
    const [, curveId, sensorId, kind] = match;
    return {
      message,
      headline: `${curveId} reads a ${kind} sensor`,
      detail: 'Only temperature and metric sensors can drive a curve.',
      raw: sensorId,
      nodeId: curveNodeRef(curveId, profile),
      curves: [curveId],
    };
  }
  match = VIRTUAL_SENSOR.exec(message);
  if (match) {
    const name = match[1];
    return { message, headline: message.charAt(0).toUpperCase() + message.slice(1), detail: '', raw: '', nodeId: `virtual:${name}`, curves: [] };
  }
  return plain(message);
}
