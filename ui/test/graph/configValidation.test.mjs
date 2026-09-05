import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  curveValidationError,
  configValidationError,
  virtualSensorCycleError,
  undefinedVirtualReferenceError,
} from '../../src/lib/graph/configValidation.js';

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

test('virtualSensorCycleError finds no cycle in an acyclic graph', () => {
  const sensors = {
    a: { type: 'max', inputs: ['hwmon/x/temp1'] },
    b: { type: 'offset', input: 'virtual/a', add: 0, scale: 1 },
  };
  assert.equal(virtualSensorCycleError(sensors), '');
});

test('virtualSensorCycleError matches the daemon message for a two-sensor cycle', () => {
  const sensors = {
    a: { type: 'max', inputs: ['virtual/b'] },
    b: { type: 'offset', input: 'virtual/a', add: 0, scale: 1 },
  };
  assert.equal(virtualSensorCycleError(sensors), 'virtual sensor cycle: a -> b -> a');
});

test('virtualSensorCycleError matches the daemon message for a self reference', () => {
  const sensors = {
    s: { type: 'min', inputs: ['virtual/s'] },
  };
  assert.equal(virtualSensorCycleError(sensors), 'virtual sensor cycle: s -> s');
});

test('undefinedVirtualReferenceError matches the daemon message for an undefined sensor input', () => {
  const sensors = {
    x: { type: 'max', inputs: ['virtual/other'] },
  };
  assert.equal(
    undefinedVirtualReferenceError({}, sensors),
    "virtual sensor 'x' references undefined virtual sensor 'virtual/other'"
  );
});

test('undefinedVirtualReferenceError catches a curve referencing an undefined virtual id', () => {
  const curves = {
    cpu: { type: 'point', sensor: 'virtual/missing', points: [], hysteresis: null, response: null },
  };
  assert.equal(
    undefinedVirtualReferenceError(curves, {}),
    "curve 'cpu' references undefined virtual sensor 'virtual/missing'"
  );
});

test('undefinedVirtualReferenceError passes when every virtual reference resolves', () => {
  const curves = {
    cpu: { type: 'point', sensor: 'virtual/a', points: [], hysteresis: null, response: null },
  };
  const sensors = { a: { type: 'max', inputs: ['hwmon/x/temp1'] } };
  assert.equal(undefinedVirtualReferenceError(curves, sensors), '');
});

test('configValidationError reports a virtual sensor cycle', () => {
  const cfg = config(
    {},
    {
      a: { type: 'max', inputs: ['virtual/b'] },
      b: { type: 'offset', input: 'virtual/a', add: 0, scale: 1 },
    }
  );
  assert.equal(configValidationError(cfg), 'virtual sensor cycle: a -> b -> a');
});

test('configValidationError reports an undefined virtual reference from a curve', () => {
  const cfg = config({
    cpu: { type: 'point', sensor: 'virtual/missing', points: [], hysteresis: null, response: null },
  });
  assert.equal(
    configValidationError(cfg),
    "curve 'cpu' references undefined virtual sensor 'virtual/missing'"
  );
});

test('curveValidationError checks hysteresis and response on linear and trigger curves too', () => {
  const linear = { type: 'linear', min_temp: 40, max_temp: 80, min_duty: 20, max_duty: 100, hysteresis: { up: 'x', down: 5 }, response: null };
  assert.match(curveValidationError('ramp', linear), /hysteresis up/);
  const trigger = { type: 'trigger', on_temp: 60, off_temp: 50, on_duty: 100, off_duty: 20, response: { rise_pct_per_sec: 10, fall_pct_per_sec: null } };
  assert.match(curveValidationError('kick', trigger), /response fall/);
  assert.equal(curveValidationError('ok', { ...trigger, response: null }), '');
});
