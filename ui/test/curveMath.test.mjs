import { test } from 'node:test';
import assert from 'node:assert/strict';
import { evalCurve, curveScale, curvePaths, sparklinePath, nextPointTemp, curvePoints, axisFor, axisSpan, axisTicks, niceCeiling, pointFieldValue, createAxisHold } from '../src/lib/curveMath.js';

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
  assert.deepEqual(axisFor('power'), { min: 0, max: 600, unit: 'W', step: 200 });
  assert.deepEqual(axisFor('clock'), { min: 0, max: 4000, unit: 'MHz', step: 1000 });
  assert.deepEqual(axisFor('memory'), { min: 0, max: 32768, unit: 'MiB', step: 10000 });
  assert.deepEqual(axisFor('state'), { min: 0, max: 15, unit: '', step: 5 });
  assert.deepEqual(axisFor('percent'), { min: 0, max: 100, unit: '%', step: 20 });
  assert.deepEqual(axisFor('temp'), { min: 0, max: 100, unit: '\u00b0C', step: 20 });
  assert.deepEqual(axisFor(undefined), { min: 0, max: 100, unit: '\u00b0C', step: 20 });
  assert.deepEqual(axisFor('clock', []), axisFor('clock'));
});

test('niceCeiling rounds a value up the ceiling ladder with 5 percent headroom', () => {
  assert.equal(niceCeiling(15532), 20000);
  assert.equal(niceCeiling(450), 500);
  assert.equal(niceCeiling(2872), 4000);
  assert.equal(niceCeiling(620), 800);
  assert.equal(niceCeiling(110), 120);
  assert.equal(niceCeiling(105), 120);
  assert.equal(niceCeiling(95), 100);
  assert.equal(niceCeiling(1), 1.2);
  assert.equal(niceCeiling(2000), 2500);
  assert.equal(niceCeiling(0), 0);
  assert.equal(niceCeiling(-5), 0);
  assert.equal(niceCeiling(Number.MAX_VALUE), 0);
  assert.equal(niceCeiling(Infinity), 0);
});

test('axisFor keeps the kind default until an observed value passes it', () => {
  assert.equal(axisFor('clock', [2872]).max, 4000);
  assert.equal(axisFor('clock', [4000]).max, 4000);
  assert.equal(axisFor('clock', [15532]).max, 20000);
  assert.equal(axisFor('clock', [1000, 15000, null, undefined, NaN]).max, 20000);
  assert.equal(axisFor('power', [450]).max, 600);
  assert.equal(axisFor('temp', [70]).max, 100);
  assert.equal(axisFor('temp', [110]).max, 120);
  assert.equal(axisFor('power', [620]).max, 800);
  assert.equal(axisFor('clock', [15532]).unit, 'MHz');
});

test('axisSpan folds curve points and the live reading into one axis', () => {
  assert.equal(axisSpan([[15000, 100]], null, 'clock').max, 20000);
  assert.equal(axisSpan([{ temp: 15000, duty: 100 }], null, 'clock').max, 20000);
  assert.equal(axisSpan([[2000, 100]], 15532, 'clock').max, 20000);
  assert.equal(axisSpan([[2000, 100]], 2500, 'clock').max, 4000);
  assert.equal(axisSpan(null, null, 'clock').max, 4000);
  assert.deepEqual(axisSpan([], null, undefined), axisFor(undefined));
});

test('axisTicks stays readable on a wide axis', () => {
  assert.deepEqual(axisTicks(axisFor('temp')), [0, 20, 40, 60, 80, 100]);
  assert.deepEqual(axisTicks(axisFor('temp', [110])), [0, 25, 50, 75, 100, 120]);
  assert.deepEqual(axisTicks(axisFor('clock', [15532])), [0, 5000, 10000, 15000, 20000]);
  const memory = axisTicks(axisFor('memory'));
  assert.ok(memory.length <= 8);
  assert.equal(memory[memory.length - 1], 32768);
});

test('pointFieldValue clamps duty but keeps an x value the user typed past the axis', () => {
  assert.equal(pointFieldValue('duty', '120'), 100);
  assert.equal(pointFieldValue('duty', '-3'), 0);
  assert.equal(pointFieldValue('temp', '18000'), 18000);
  assert.equal(pointFieldValue('temp', '-4'), -4);
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

test('an axis hold grows with the curve and never shrinks back while the same curve is shown', () => {
  const hold = createAxisHold();
  assert.equal(hold('curve:cpu', [[70, 100]], 60, 'temp').max, 100);
  assert.equal(hold('curve:cpu', [[70, 100]], 105, 'temp').max, 120);
  assert.equal(hold('curve:cpu', [[70, 100]], 60, 'temp').max, 120);
});

test('an axis hold does not flip while a live value hovers around the previous max', () => {
  const hold = createAxisHold();
  const seen = [99.5, 100, 100.4, 99.8, 100.2].map((live) => hold('curve:cpu', [[70, 100]], live, 'temp').max);
  assert.deepEqual(seen, [120, 120, 120, 120, 120]);
});

test('an axis hold keeps its ticks in step with the held max and forgets a curve that changes kind', () => {
  const hold = createAxisHold();
  hold('curve:cpu', [[15000, 100]], null, 'clock');
  const held = hold('curve:cpu', [[3000, 100]], null, 'clock');
  assert.equal(held.max, 20000);
  assert.deepEqual(axisTicks(held), [0, 5000, 10000, 15000, 20000]);
  assert.equal(hold('curve:cpu', [[70, 100]], null, 'temp').max, 100);
  assert.equal(hold('curve:other', [[70, 100]], null, 'temp').max, 100);
});
