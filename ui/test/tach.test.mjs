import { test } from 'node:test';
import assert from 'node:assert/strict';
import { tachSensorFor } from '../src/lib/tach.js';

const sensors = [
  { id: 'hwmon/nct6798/temp1', kind: 'temp' },
  { id: 'hwmon/nct6798/fan1', kind: 'rpm' },
  { id: 'hwmon/nct6798/fan2', kind: 'rpm' },
  { id: 'nvidia/0/fan', kind: 'rpm' },
  { id: 'corsair/cp/fan3', kind: 'rpm' },
  { id: 'corsair/cp/fan4', kind: 'rpm' },
];

test('pairs by matching channel number on the same device', () => {
  assert.equal(tachSensorFor('hwmon/nct6798/pwm2', sensors).id, 'hwmon/nct6798/fan2');
});

test('pairs by exact id when the control and tach share one', () => {
  assert.equal(tachSensorFor('corsair/cp/fan3', sensors).id, 'corsair/cp/fan3');
});

test('pairs the only rpm sensor on a device regardless of name', () => {
  assert.equal(tachSensorFor('nvidia/0/fan', sensors).id, 'nvidia/0/fan');
});

test('returns null when nothing on the device matches', () => {
  assert.equal(tachSensorFor('hwmon/nct6798/pwm5', sensors), null);
  assert.equal(tachSensorFor('other/dev/pwm1', sensors), null);
});
