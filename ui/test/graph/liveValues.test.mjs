import { test } from 'node:test';
import assert from 'node:assert/strict';
import { formatDeltaRate, isOverridden, sensorDisplay, curveOutput, isCurveInputKind } from '../../src/lib/graph/liveValues.js';

test('formatDeltaRate formats a rate in degrees per minute', () => {
  assert.equal(formatDeltaRate(2.5), '2.5 C/min');
});

test('formatDeltaRate formats a negative rate', () => {
  assert.equal(formatDeltaRate(-1.25), '-1.3 C/min');
});

test('formatDeltaRate handles missing values', () => {
  assert.equal(formatDeltaRate(null), 'n/a');
  assert.equal(formatDeltaRate(undefined), 'n/a');
});

test('isOverridden is true when the handle is in the snapshot overrides list', () => {
  assert.equal(isOverridden({ overrides: ['pwm1'] }, 'pwm1'), true);
});

test('isOverridden is false when the handle is not in the snapshot overrides list', () => {
  assert.equal(isOverridden({ overrides: ['pwm1'] }, 'pwm2'), false);
});

test('isOverridden handles a missing or overrides-less snapshot', () => {
  assert.equal(isOverridden(null, 'pwm1'), false);
  assert.equal(isOverridden({}, 'pwm1'), false);
});

test('sensorDisplay splits a reading into text and unit by kind', () => {
  assert.deepEqual(sensorDisplay(45.26, 'temp'), { text: '45.3', unit: '°C' });
  assert.deepEqual(sensorDisplay(45.26, undefined), { text: '45.3', unit: '°C' });
  assert.deepEqual(sensorDisplay(1199.6, 'rpm'), { text: '1200', unit: 'rpm' });
  assert.deepEqual(sensorDisplay(45.4, 'duty'), { text: '45', unit: '%' });
  assert.deepEqual(sensorDisplay(null, 'duty'), { text: '—', unit: '%' });
});

test('sensorDisplay knows every kind', () => {
  assert.deepEqual(sensorDisplay(312.46, 'power'), { text: '312.5', unit: 'W' });
  assert.deepEqual(sensorDisplay(2505, 'clock'), { text: '2505', unit: 'MHz' });
  assert.deepEqual(sensorDisplay(4096.4, 'memory'), { text: '4096', unit: 'MiB' });
  assert.deepEqual(sensorDisplay(37, 'percent'), { text: '37', unit: '%' });
  assert.deepEqual(sensorDisplay(2, 'state'), { text: 'P2', unit: '' });
  assert.deepEqual(sensorDisplay(null, 'state'), { text: '—', unit: '' });
});

test('isCurveInputKind excludes only rpm and duty', () => {
  for (const kind of ['temp', 'percent', 'clock', 'memory', 'power', 'state', undefined]) {
    assert.equal(isCurveInputKind(kind), true, String(kind));
  }
  assert.equal(isCurveInputKind('rpm'), false);
  assert.equal(isCurveInputKind('duty'), false);
});

test('curveOutput reads the daemon-published output for a curve id', () => {
  assert.equal(curveOutput({ curves: { cpu: 42.4 } }, 'cpu'), 42.4);
});

test('curveOutput is null for a missing curve, a missing map, or no snapshot', () => {
  assert.equal(curveOutput({ curves: {} }, 'cpu'), null);
  assert.equal(curveOutput({ duties: { pwm1: 10 } }, 'cpu'), null);
  assert.equal(curveOutput(null, 'cpu'), null);
});
