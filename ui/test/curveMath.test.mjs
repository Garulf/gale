import { test } from 'node:test';
import assert from 'node:assert/strict';
import { evalCurve, curveScale, curvePaths, sparklinePath, nextPointTemp, curvePoints, axisFor } from '../src/lib/curveMath.js';

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

test('axisFor maps kinds to editor domains and falls back to temperature', () => {
  assert.deepEqual(axisFor('power'), { max: 600, unit: 'W' });
  assert.deepEqual(axisFor('clock'), { max: 4000, unit: 'MHz' });
  assert.deepEqual(axisFor('memory'), { max: 32768, unit: 'MiB' });
  assert.deepEqual(axisFor('state'), { max: 15, unit: '' });
  assert.deepEqual(axisFor('percent'), { max: 100, unit: '%' });
  assert.deepEqual(axisFor('temp'), { max: 100, unit: '°C' });
  assert.deepEqual(axisFor(undefined), { max: 100, unit: '°C' });
});

test('curveScale and curvePaths honour a wider x domain', () => {
  const scale = curveScale(200, 100, 0, 400);
  assert.equal(scale.x(400), 200);
  assert.equal(scale.x(100), 50);
  const paths = curvePaths([[0, 0], [400, 100]], 200, 100, 0, 400);
  assert.ok(paths.line.endsWith('L 200.0 0.0'));
});

test('nextPointTemp honours a wider x domain for its outer bound', () => {
  assert.equal(nextPointTemp([100, 300], 400), 200);
  assert.equal(nextPointTemp([], 400), 200);
});

test('curvePoints exposes point curves as-is and linear curves as their two ends', () => {
  assert.deepEqual(curvePoints({ type: 'point', points: [[30, 20]] }), [[30, 20]]);
  assert.deepEqual(curvePoints({ type: 'linear', min_temp: 40, max_temp: 80, min_duty: 20, max_duty: 100 }), [[40, 20], [80, 100]]);
  assert.equal(curvePoints({ type: 'flat', duty: 50 }), null);
  assert.equal(curvePoints(null), null);
  assert.equal(evalCurve(curvePoints({ type: 'linear', min_temp: 40, max_temp: 80, min_duty: 20, max_duty: 100 }), 60), 60);
});
