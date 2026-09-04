import { test } from 'node:test';
import assert from 'node:assert/strict';
import { curveValidationError, configValidationError } from '../../src/lib/graph/configValidation.js';

function config(curves, sensors = {}) {
  return { profiles: { default: { curves, assignments: {}, sensors } } };
}

test('a fully numeric config passes', () => {
  const cfg = config(
    {
      cpu: { type: 'point', sensor: 'a', points: [[30, 20], [70, 100]], hysteresis: { up: 2, down: 5 }, response: null },
      flat: { type: 'flat', duty: 50 },
      trig: { type: 'trigger', sensor: 'a', on_temp: 60, off_temp: 50, on_duty: 100, off_duty: 20 },
      tgt: { type: 'target', sensor: 'a', target_temp: 60, step_pct_per_sec: 5, min_duty: 20, max_duty: 100 },
      mix: { type: 'mix', sources: ['cpu'], mode: 'max' },
    },
    { hot: { type: 'max', inputs: ['a'] } }
  );
  assert.equal(configValidationError(cfg), '');
});

test('a cleared flat duty reports the old Curves page message', () => {
  assert.equal(curveValidationError('x', { type: 'flat', duty: null }), 'Curve "x": duty must be a number');
  assert.equal(curveValidationError('x', { type: 'flat', duty: NaN }), 'Curve "x": duty must be a number');
});

test('point curve messages match the old Curves page', () => {
  assert.equal(
    curveValidationError('cpu', { type: 'point', points: [[null, 20]], hysteresis: null, response: null }),
    'Curve "cpu": a point\'s temperature must be a number'
  );
  assert.equal(
    curveValidationError('cpu', { type: 'point', points: [[30, 20]], hysteresis: { up: null, down: 5 }, response: null }),
    'Curve "cpu": hysteresis up must be a number'
  );
  assert.equal(
    curveValidationError('cpu', { type: 'point', points: [[30, 20]], hysteresis: null, response: { rise_pct_per_sec: 10, fall_pct_per_sec: null } }),
    'Curve "cpu": response fall %/s must be a number'
  );
});

test('trigger and target messages name the field', () => {
  assert.equal(
    curveValidationError('t', { type: 'trigger', on_temp: 60, off_temp: null, on_duty: 100, off_duty: 20 }),
    'Curve "t": off_temp must be a number'
  );
  assert.equal(
    curveValidationError('g', { type: 'target', target_temp: 60, step_pct_per_sec: 5, min_duty: 20, max_duty: undefined }),
    'Curve "g": max_duty must be a number'
  );
});

test('configValidationError falls through to virtual sensor validation', () => {
  const cfg = config({ flat: { type: 'flat', duty: 50 } }, { hot: { type: 'max', inputs: [] } });
  assert.equal(configValidationError(cfg), 'Sensor "hot": at least one input is required');
});

test('configValidationError reports the first curve problem before sensors', () => {
  const cfg = config({ flat: { type: 'flat', duty: null } }, { hot: { type: 'max', inputs: [] } });
  assert.equal(configValidationError(cfg), 'Curve "flat": duty must be a number');
});
