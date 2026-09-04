import { test } from 'node:test';
import assert from 'node:assert/strict';
import { autoLayout } from '../../src/lib/graph/layout.js';
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
