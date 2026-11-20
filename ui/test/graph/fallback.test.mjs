import { test } from 'node:test';
import assert from 'node:assert/strict';
import { fallbackCurveIds, fallbackControlIds, portState } from '../../src/lib/graph/liveValues.js';

const profile = {
  curves: { exhaust_bias: {}, cpu_response: {} },
  assignments: { 'hwmon/it8792/pwm3': 'exhaust_bias', 'hwmon/it8792/pwm1': 'cpu_response' },
};

test('a saved curve missing from the snapshot curve outputs is in fallback', () => {
  const snap = { active_profile: 'MissingProbe', curves: { cpu_response: 48 } };
  assert.deepEqual([...fallbackCurveIds(snap, 'MissingProbe', profile)], ['exhaust_bias']);
});

test('no fallback is reported for another profile or without a snapshot', () => {
  const snap = { active_profile: 'Workstation', curves: {} };
  assert.equal(fallbackCurveIds(snap, 'MissingProbe', profile).size, 0);
  assert.equal(fallbackCurveIds(null, 'MissingProbe', profile).size, 0);
  assert.equal(fallbackCurveIds(snap, 'Workstation', null).size, 0);
});

test('controls assigned to a fallback curve are in fallback', () => {
  const ids = fallbackControlIds(new Set(['exhaust_bias']), profile);
  assert.deepEqual([...ids], ['hwmon/it8792/pwm3']);
  assert.equal(fallbackControlIds(new Set(), profile).size, 0);
});

test('portState separates spare, unavailable and connected ports', () => {
  assert.equal(portState(false, null), 'spare');
  assert.equal(portState(true, null), 'unavailable');
  assert.equal(portState(true, undefined), 'unavailable');
  assert.equal(portState(true, 0), 'connected');
});
