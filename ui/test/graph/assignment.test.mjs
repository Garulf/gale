import { test } from 'node:test';
import assert from 'node:assert/strict';
import { edgeInto, isSingleInputHandle, replaceEdge } from '../../src/lib/graph/model.js';
import { numberedInputCount, virtualInputHandles } from '../../src/lib/graph/ids.js';

function edge(source, sourceHandle, target, targetHandle, kind) {
  return {
    id: `${source}:${sourceHandle}->${target}:${targetHandle}`,
    source,
    sourceHandle,
    target,
    targetHandle,
    type: 'gale',
    class: kind,
    data: { kind },
  };
}

function byId(a, b) {
  return a.id.localeCompare(b.id);
}

const unrelated = [
  edge('sensor:hwmon/x', 'hwmon/x/temp1', 'virtual:hot', 'in-0', 'temp'),
  edge('sensor:hwmon/x', 'hwmon/x/temp2', 'virtual:hot', 'in-1', 'temp'),
  edge('curve:cpu', 'out', 'control:hwmon/x', 'hwmon/x/pwm1', 'duty'),
];

test('edgeInto finds the edge occupying a target handle among unrelated edges', () => {
  const wanted = edge('virtual:hot', 'out', 'curve:cpu', 'sensor', 'temp');
  const edges = [unrelated[0], wanted, unrelated[1], unrelated[2]];
  assert.equal(edgeInto(edges, 'curve:cpu', 'sensor'), wanted);
});

test('edgeInto returns undefined when nothing targets the handle', () => {
  assert.equal(edgeInto(unrelated, 'curve:cpu', 'sensor'), undefined);
  assert.equal(edgeInto(unrelated, 'virtual:hot', 'in-2'), undefined);
  assert.equal(edgeInto([], 'curve:cpu', 'sensor'), undefined);
});

test('isSingleInputHandle is true for curve sensor, combine in and virtual in', () => {
  assert.equal(isSingleInputHandle('curve', 'sensor'), true);
  assert.equal(isSingleInputHandle('combine', 'in'), true);
  assert.equal(isSingleInputHandle('virtual', 'in'), true);
});

test('isSingleInputHandle is false for numbered, control and device handles', () => {
  assert.equal(isSingleInputHandle('virtual', 'in-0'), false);
  assert.equal(isSingleInputHandle('virtual', 'in-3'), false);
  assert.equal(isSingleInputHandle('combine', 'in-1'), false);
  assert.equal(isSingleInputHandle('control', 'hwmon/nct6798/pwm1'), false);
  assert.equal(isSingleInputHandle('deviceControl', 'hwmon/nct6798/pwm1'), false);
  assert.equal(isSingleInputHandle('deviceControl', 'in'), false);
});

test('replaceEdge removes only the edge into the handle and appends the new one', () => {
  const kept = edge('curve:cpu', 'out', 'control:hwmon/x', 'hwmon/x/pwm1', 'duty');
  const old = edge('sensor:hwmon/x', 'hwmon/x/temp1', 'curve:cpu', 'sensor', 'temp');
  const replacement = edge('virtual:hot', 'out', 'curve:cpu', 'sensor', 'temp');
  const result = replaceEdge([old, kept], replacement, 'curve:cpu', 'sensor');
  assert.deepEqual(result.slice().sort(byId), [kept, replacement].sort(byId));
  assert.equal(result[result.length - 1], replacement);
});

test('replaceEdge on an empty handle just appends', () => {
  const added = edge('virtual:hot', 'out', 'curve:cpu', 'sensor', 'temp');
  assert.deepEqual(replaceEdge(unrelated, added, 'curve:cpu', 'sensor'), [...unrelated, added]);
});

test('numberedInputCount counts connected in-N handles and covers the highest index', () => {
  assert.equal(numberedInputCount([]), 0);
  assert.equal(numberedInputCount(['in-0']), 1);
  assert.equal(numberedInputCount(['in-0', 'in-1']), 2);
  assert.equal(numberedInputCount(['in-1']), 2);
  assert.equal(numberedInputCount(['in-0', 'in-2']), 3);
  assert.equal(numberedInputCount(['in', 'sensor']), 0);
});

test('a growing node always shows one more handle than it has connections', () => {
  assert.deepEqual(virtualInputHandles('max', numberedInputCount([]) + 1), ['in-0']);
  assert.deepEqual(virtualInputHandles('max', numberedInputCount(['in-0']) + 1), ['in-0', 'in-1']);
  assert.deepEqual(virtualInputHandles('mean', numberedInputCount(['in-0', 'in-1']) + 1), ['in-0', 'in-1', 'in-2']);
  assert.deepEqual(virtualInputHandles('offset', numberedInputCount(['in']) + 1), ['in']);
});
