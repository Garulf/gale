import { test } from 'node:test';
import assert from 'node:assert/strict';
import { evalCurve, curvePaths, sparklinePath, nextPointTemp, curvePoints } from '../src/lib/curveMath.js';

const points = [
  [70, 100],
  [30, 20],
];

test('evalCurve interpolates and clamps at the ends', () => {
  assert.equal(evalCurve(points, 10), 20);
  assert.equal(evalCurve(points, 50), 60);
  assert.equal(evalCurve(points, 90), 100);
  assert.equal(evalCurve([], 50), null);
});

test('curvePaths extends flat to both edges and closes the area', () => {
  const { line, area } = curvePaths(points, 100, 100);
  assert.equal(line, 'M 0.0 80.0 L 30.0 80.0 L 70.0 0.0 L 100.0 0.0');
  assert.ok(area.startsWith(line));
  assert.ok(area.endsWith('L 100.0 100.0 L 0.0 100.0 Z'));
});

test('sparklinePath normalizes values into the box', () => {
  assert.equal(sparklinePath([1], 100, 20), '');
  assert.equal(sparklinePath([0, 10], 100, 20), 'M 0.0 18.0 L 100.0 2.0');
});

test('nextPointTemp picks the middle of the widest gap and never lands on an existing point', () => {
  assert.equal(nextPointTemp([30, 70]), 50);
  assert.equal(nextPointTemp([50]), 25);
  assert.equal(nextPointTemp([49, 50]), 75);
  assert.equal(nextPointTemp([0, 100]), 50);
  assert.equal(nextPointTemp([]), 50);
});

test('curvePoints exposes point curves as-is and linear curves as their two ends', () => {
  assert.deepEqual(curvePoints({ type: 'point', points: [[30, 20]] }), [[30, 20]]);
  assert.deepEqual(curvePoints({ type: 'linear', min_temp: 40, max_temp: 80, min_duty: 20, max_duty: 100 }), [[40, 20], [80, 100]]);
  assert.equal(curvePoints({ type: 'flat', duty: 50 }), null);
  assert.equal(curvePoints(null), null);
  assert.equal(evalCurve(curvePoints({ type: 'linear', min_temp: 40, max_temp: 80, min_duty: 20, max_duty: 100 }), 60), 60);
});
