import { test } from 'node:test';
import assert from 'node:assert/strict';
import { autoLayout, measure } from '../../src/lib/graph/layout.js';
import { configToGraph } from '../../src/lib/graph/model.js';

function fixtureBConfig() {
  return {
    tick_interval_ms: 1000,
    active_profile: 'default',
    profiles: {
      default: {
        sensors: {
          hot: { type: 'max', inputs: ['hwmon/chipA/temp1', 'nvidia/0/temp'] },
        },
        curves: {
          cpu: { type: 'point', sensor: 'virtual/hot', points: [[30, 20], [70, 100]] },
          baseline: { type: 'flat', duty: 40 },
          blend: { type: 'mix', sources: ['cpu', 'baseline'], mode: 'max' },
          follow: { type: 'sync', source: 'blend' },
        },
        assignments: {
          'hwmon/chipA/pwm1': 'cpu',
          'corsair/dev1/fan1': 'follow',
        },
      },
    },
  };
}

function fixtureBInventory() {
  return {
    sensors: [
      { id: 'hwmon/chipA/temp1', label: 'CPU', kind: 'temp' },
      { id: 'nvidia/0/temp', label: 'GPU', kind: 'temp' },
    ],
    controls: [
      { id: 'hwmon/chipA/pwm1', label: 'CPU fan' },
      { id: 'corsair/dev1/fan1', label: 'Case fan' },
    ],
  };
}

test('autoLayout keeps x non-decreasing along every source-to-target edge', () => {
  const { nodes, edges } = configToGraph(fixtureBConfig(), fixtureBInventory(), 'default');
  const laidOut = autoLayout(nodes, edges);
  const positionById = new Map(laidOut.map((node) => [node.id, node.position]));

  for (const edge of edges) {
    const sourceX = positionById.get(edge.source).x;
    const targetX = positionById.get(edge.target).x;
    assert.ok(sourceX <= targetX, `${edge.id}: source x ${sourceX} > target x ${targetX}`);
  }
});

test('autoLayout preserves each node id, type and data', () => {
  const { nodes, edges } = configToGraph(fixtureBConfig(), fixtureBInventory(), 'default');
  const laidOut = autoLayout(nodes, edges);

  assert.equal(laidOut.length, nodes.length);
  const originalById = new Map(nodes.map((node) => [node.id, node]));
  for (const node of laidOut) {
    const original = originalById.get(node.id);
    assert.ok(original);
    assert.equal(node.type, original.type);
    assert.deepEqual(node.data, original.data);
  }
});

test('measure grows device nodes with every wired row, not just the first three', () => {
  const rows = (count) => Array.from({ length: count }, (_, i) => ({ handle: `fan${i}`, label: `fan${i}`, wired: true }));
  const three = { id: 'control:a', type: 'deviceControl', data: { deviceControl: { device: 'a', rows: rows(3) } } };
  const six = { id: 'control:b', type: 'deviceControl', data: { deviceControl: { device: 'b', rows: rows(6) } } };
  assert.ok(measure(six, []).height > measure(three, []).height);
});

test('measure sizes combine nodes by their live incoming edges', () => {
  const mix = { id: 'combine:m', type: 'combine', data: { combine: { id: 'm', config: { type: 'mix', mode: 'max' } } } };
  const edges = Array.from({ length: 5 }, (_, i) => ({ source: `curve:c${i}`, sourceHandle: 'out', target: 'combine:m', targetHandle: `in-${i}` }));
  const single = [edges[0]];
  assert.ok(measure(mix, edges).height > measure(mix, single).height);
  assert.ok(measure(mix, single).height > measure(mix, []).height);
});
