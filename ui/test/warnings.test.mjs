import { test } from 'node:test';
import assert from 'node:assert/strict';
import { pruneDismissed, describeWarning } from '../src/lib/warnings.js';

test('pruneDismissed forgets dismissals whose warning has cleared', () => {
  const dismissed = ['sensor a missing', 'sensor b missing'];
  assert.deepEqual(pruneDismissed(dismissed, ['sensor b missing', 'new warning']), ['sensor b missing']);
  assert.deepEqual(pruneDismissed(dismissed, []), []);
});

const profile = {
  curves: {
    exhaust_bias: { type: 'point', sensor: 'hwmon/it8792/temp99', points: [] },
    cpu_response: { type: 'point', sensor: 'hwmon/it8792/temp1', points: [] },
    spare_bias: { type: 'point', sensor: 'hwmon/it8792/temp98', points: [] },
    draft: { type: 'point', sensor: '', points: [] },
    tach: { type: 'point', sensor: 'hwmon/it8792/fan1', points: [] },
    blend: { type: 'mix', sources: ['cpu_response'], mode: 'max' },
  },
  sensors: { probe: { type: 'max', inputs: ['hwmon/it8792/temp2', 'hwmon/it8792/temp97'] } },
  assignments: { 'hwmon/it8792/pwm3': 'exhaust_bias', 'hwmon/it8792/pwm1': 'cpu_response', 'hwmon/it8792/pwm4': 'draft' },
};
const labels = { 'hwmon/it8792/pwm3': 'Rear exhaust' };

test('a missing sensor leads with the fan it forces to the fail-safe duty and targets the curve', () => {
  const entry = describeWarning('referenced sensor not found on hardware: hwmon/it8792/temp99', { profile, labels });
  assert.equal(entry.headline, 'Rear exhaust is running at 100%');
  assert.equal(entry.raw, 'hwmon/it8792/temp99');
  assert.equal(entry.nodeId, 'curve:exhaust_bias');
  assert.deepEqual(entry.curves, ['exhaust_bias']);
  assert.match(entry.detail, /exhaust_bias/);
});

test('the live duty replaces the assumed fail-safe value when the snapshot has it', () => {
  const entry = describeWarning('referenced sensor not found on hardware: hwmon/it8792/temp99', {
    profile,
    labels,
    duties: { 'hwmon/it8792/pwm3': 80 },
  });
  assert.equal(entry.headline, 'Rear exhaust is running at 80%');
});

test('a missing sensor on an unassigned curve says the curve has no input', () => {
  const entry = describeWarning('referenced sensor not found on hardware: hwmon/it8792/temp98', { profile, labels });
  assert.equal(entry.headline, 'spare_bias has no input');
  assert.equal(entry.nodeId, 'curve:spare_bias');
});

test('a missing virtual sensor input targets the virtual sensor', () => {
  const entry = describeWarning('referenced sensor not found on hardware: hwmon/it8792/temp97', { profile, labels });
  assert.equal(entry.headline, 'probe is missing an input');
  assert.equal(entry.nodeId, 'virtual:probe');
});

test('an unwired curve names its fan, using the id tail when there is no label', () => {
  const entry = describeWarning("curve 'draft' has no input wired and will fail safe to 100%", { profile, labels });
  assert.equal(entry.headline, 'pwm4 is running at 100%');
  assert.equal(entry.nodeId, 'curve:draft');
});

test('a curve reading a non-input sensor targets that curve', () => {
  const entry = describeWarning("curve 'tach' reads 'hwmon/it8792/fan1', a rpm sensor that cannot drive a curve", { profile, labels });
  assert.equal(entry.headline, 'tach reads a rpm sensor');
  assert.equal(entry.nodeId, 'curve:tach');
  assert.equal(entry.raw, 'hwmon/it8792/fan1');
});

test('combine curves resolve to their combine node id', () => {
  const entry = describeWarning("curve 'blend' has no input wired and will fail safe to 100%", { profile, labels });
  assert.equal(entry.nodeId, 'combine:blend');
});

test('unknown warnings and a missing profile pass the message through untouched', () => {
  const platform = describeWarning('PawnIO driver is not installed', { profile, labels });
  assert.equal(platform.headline, 'PawnIO driver is not installed');
  assert.equal(platform.nodeId, null);
  const bare = describeWarning('referenced sensor not found on hardware: hwmon/it8792/temp99');
  assert.equal(bare.headline, 'referenced sensor not found on hardware: hwmon/it8792/temp99');
});
