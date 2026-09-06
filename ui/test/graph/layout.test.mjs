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

function fixtureScatteredDevices() {
  const config = {
    tick_interval_ms: 1000,
    active_profile: 'default',
    profiles: {
      default: {
        sensors: {
          hot: { type: 'max', inputs: ['hwmon/chipA/temp1'] },
          smooth: { type: 'mean', inputs: ['virtual/hot'], window_s: 10 },
        },
        curves: {
          deep: { type: 'point', sensor: 'virtual/smooth', points: [[30, 20], [70, 100]] },
          gpu: { type: 'point', sensor: 'nvidia/0/temp', points: [[30, 20], [70, 100]] },
          blend: { type: 'mix', sources: ['deep', 'gpu'], mode: 'max' },
        },
        assignments: { 'corsair/dev1/fan1': 'blend' },
      },
    },
  };
  const inventory = {
    sensors: [
      { id: 'hwmon/chipA/temp1', label: 'CPU', kind: 'temp' },
      { id: 'nvidia/0/temp', label: 'GPU', kind: 'temp' },
      { id: 'ec/board/vrm', label: 'VRM', kind: 'temp' },
    ],
    controls: [
      { id: 'corsair/dev1/fan1', label: 'Case fan' },
      { id: 'hwmon/chipA/pwm1', label: 'Unwired header' },
      { id: 'nvidia/0/fan0', label: 'GPU fan' },
    ],
  };
  return configToGraph(config, inventory, 'default');
}

test('autoLayout pins every device sensor left of, and every device control right of, all other nodes', () => {
  const { nodes, edges } = fixtureScatteredDevices();
  const laidOut = autoLayout(nodes, edges);
  const xs = (type) => laidOut.filter((node) => node.type === type).map((node) => node.position.x);
  const middle = laidOut.filter((node) => node.type !== 'deviceSensor' && node.type !== 'deviceControl');

  assert.equal(xs('deviceSensor').length, 3);
  assert.equal(xs('deviceControl').length, 3);
  assert.ok(middle.length > 0);
  const sensorRight = Math.max(...xs('deviceSensor'));
  const controlLeft = Math.min(...xs('deviceControl'));
  for (const node of middle) {
    assert.ok(node.position.x > sensorRight, `${node.id} at x ${node.position.x} is not right of the sensor column (${sensorRight})`);
    assert.ok(node.position.x < controlLeft, `${node.id} at x ${node.position.x} is not left of the control column (${controlLeft})`);
  }
});

test('autoLayout stacks a device column without overlap', () => {
  const { nodes, edges } = fixtureScatteredDevices();
  const laidOut = autoLayout(nodes, edges);
  for (const type of ['deviceSensor', 'deviceControl']) {
    const column = laidOut.filter((node) => node.type === type).sort((a, b) => a.position.y - b.position.y);
    for (let i = 1; i < column.length; i += 1) {
      const above = column[i - 1];
      const bottomOfAbove = above.position.y + measure(above, edges).height;
      assert.ok(column[i].position.y >= bottomOfAbove, `${column[i].id} overlaps ${above.id}`);
    }
  }
});

import { projectScope } from '../../src/lib/graph/groups.js';

function groupedFixture() {
  const { nodes, edges } = fixtureScatteredDevices();
  const groups = [{ id: 'g1', name: 'Deep', position: { x: 400, y: 40 }, members: ['virtual:hot', 'virtual:smooth', 'curve:deep'], parent: null }];
  return { nodes, edges, groups };
}

test('measure sizes group nodes by their larger port side and port nodes as one row', () => {
  const { nodes, edges, groups } = groupedFixture();
  const root = projectScope(nodes, edges, groups, null);
  const group = root.nodes.find((node) => node.type === 'group');
  assert.equal(measure(group, root.edges).height, 52 + 12 + (Math.max(group.data.group.inputs.length, group.data.group.outputs.length) + 1) * 26);

  const inside = projectScope(nodes, edges, groups, 'g1');
  const port = inside.nodes.find((node) => node.type === 'port');
  assert.equal(measure(port, inside.edges).height, 52 + 12 + 2 * 26);
});

test('autoLayout inside a group pins input ports left and output ports right of the members', () => {
  const { nodes, edges, groups } = groupedFixture();
  const inside = projectScope(nodes, edges, groups, 'g1');
  const laidOut = autoLayout(inside.nodes, inside.edges);
  const members = laidOut.filter((node) => node.type !== 'port');
  const inputs = laidOut.filter((node) => node.type === 'port' && node.data.port.direction === 'in');
  const outputs = laidOut.filter((node) => node.type === 'port' && node.data.port.direction === 'out');
  assert.ok(inputs.length > 0 && outputs.length > 0);
  for (const port of inputs) for (const member of members) assert.ok(port.position.x < member.position.x, `${port.id} not left of ${member.id}`);
  for (const port of outputs) for (const member of members) assert.ok(port.position.x > member.position.x, `${port.id} not right of ${member.id}`);
});
