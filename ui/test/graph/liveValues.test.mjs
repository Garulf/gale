import { test } from 'node:test';
import assert from 'node:assert/strict';
import { formatDeltaRate, isOverridden } from '../../src/lib/graph/liveValues.js';

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
