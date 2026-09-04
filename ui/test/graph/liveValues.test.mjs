import { test } from 'node:test';
import assert from 'node:assert/strict';
import { formatDeltaRate } from '../../src/lib/graph/liveValues.js';

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
