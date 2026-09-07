import { test } from 'node:test';
import assert from 'node:assert/strict';
import { isValidConnection, wouldCycle } from '../../src/lib/graph/validate.js';
import { configToGraph } from '../../src/lib/graph/model.js';

function fixtureBConfig() {
  return {
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

const nodes = [
  { id: 'sensor:hwmon/x', type: 'deviceSensor', data: {} },
  { id: 'virtual:cpu_hot', type: 'virtual', data: {} },
  { id: 'curve:cpu', type: 'curve', data: {} },
  { id: 'combine:m', type: 'combine', data: {} },
  { id: 'control:hwmon/x', type: 'deviceControl', data: {} },
];

test('a temp source into a duty-only control handle is invalid', () => {
  const connection = {
    source: 'sensor:hwmon/x',
    sourceHandle: 'hwmon/x/temp1',
    target: 'control:hwmon/x',
    targetHandle: 'hwmon/x/pwm1',
  };
  assert.equal(isValidConnection(connection, nodes, []), false);
});

test('a duty source into a virtual sensor input is invalid', () => {
  const connection = {
    source: 'curve:cpu',
    sourceHandle: 'out',
    target: 'virtual:cpu_hot',
    targetHandle: 'in-0',
  };
  assert.equal(isValidConnection(connection, nodes, []), false);
});

test('a second edge into an already-wired single-input handle is still reported valid', () => {
  const existingEdges = [
    {
      id: 'e1',
      source: 'sensor:hwmon/x',
      sourceHandle: 'hwmon/x/temp1',
      target: 'curve:cpu',
      targetHandle: 'sensor',
      data: { kind: 'temp' },
    },
  ];
  const connection = {
    source: 'sensor:hwmon/x',
    sourceHandle: 'hwmon/x/temp1',
    target: 'curve:cpu',
    targetHandle: 'sensor',
  };
  assert.equal(isValidConnection(connection, nodes, existingEdges), true);
});

test('a self-loop is invalid', () => {
  const connection = {
    source: 'virtual:cpu_hot',
    sourceHandle: 'out',
    target: 'virtual:cpu_hot',
    targetHandle: 'in-0',
  };
  assert.equal(isValidConnection(connection, nodes, []), false);
});

test('wouldCycle detects a cycle between two virtual sensors', () => {
  const edges = [
    {
      id: 'a-b',
      source: 'virtual:a',
      sourceHandle: 'out',
      target: 'virtual:b',
      targetHandle: 'in',
      data: { kind: 'temp' },
    },
  ];
  const candidate = {
    source: 'virtual:b',
    sourceHandle: 'out',
    target: 'virtual:a',
    targetHandle: 'in',
  };
  assert.equal(wouldCycle(candidate, edges), true);
});

test('wouldCycle is false for every existing edge in fixture B (acyclic graph)', () => {
  const { nodes: fixtureNodes, edges } = configToGraph(fixtureBConfig(), fixtureBInventory(), 'default');
  for (const edge of edges) {
    const candidate = {
      source: edge.source,
      sourceHandle: edge.sourceHandle,
      target: edge.target,
      targetHandle: edge.targetHandle,
    };
    assert.equal(wouldCycle(candidate, edges), false, `${edge.id} should not report a cycle`);
  }
  assert.ok(fixtureNodes.length > 0);
});

const nodesWithSensorKinds = [
  {
    id: 'sensor:hwmon/x',
    type: 'deviceSensor',
    data: {
      deviceSensor: {
        device: 'hwmon/x',
        rows: [
          { handle: 'hwmon/x/temp1', label: 'CPU', kind: 'temp', wired: false },
          { handle: 'hwmon/x/fan1', label: 'CPU fan', kind: 'rpm', wired: false },
        ],
      },
    },
  },
  { id: 'virtual:cpu_hot', type: 'virtual', data: {} },
  { id: 'curve:cpu', type: 'curve', data: {} },
];

test('an rpm sensor row into a curve sensor input is invalid', () => {
  const connection = {
    source: 'sensor:hwmon/x',
    sourceHandle: 'hwmon/x/fan1',
    target: 'curve:cpu',
    targetHandle: 'sensor',
  };
  assert.equal(isValidConnection(connection, nodesWithSensorKinds, []), false);
});

test('an rpm sensor row into a virtual sensor input is invalid', () => {
  const connection = {
    source: 'sensor:hwmon/x',
    sourceHandle: 'hwmon/x/fan1',
    target: 'virtual:cpu_hot',
    targetHandle: 'in-0',
  };
  assert.equal(isValidConnection(connection, nodesWithSensorKinds, []), false);
});

test('a temp sensor row into a curve sensor input stays valid', () => {
  const connection = {
    source: 'sensor:hwmon/x',
    sourceHandle: 'hwmon/x/temp1',
    target: 'curve:cpu',
    targetHandle: 'sensor',
  };
  assert.equal(isValidConnection(connection, nodesWithSensorKinds, []), true);
});

const nodesWithPowerKind = [
  {
    id: 'sensor:hwmon/y',
    type: 'deviceSensor',
    data: {
      deviceSensor: {
        device: 'hwmon/y',
        rows: [{ handle: 'hwmon/y/power1', label: 'Package power', kind: 'power', wired: false }],
      },
    },
  },
  { id: 'curve:cpu', type: 'curve', data: {} },
];

test('a power sensor row into a curve sensor input is valid', () => {
  const connection = {
    source: 'sensor:hwmon/y',
    sourceHandle: 'hwmon/y/power1',
    target: 'curve:cpu',
    targetHandle: 'sensor',
  };
  assert.equal(isValidConnection(connection, nodesWithPowerKind, []), true);
});

test('a sensor row with no kind information is still accepted', () => {
  const connection = {
    source: 'sensor:hwmon/x',
    sourceHandle: 'hwmon/x/temp1',
    target: 'curve:cpu',
    targetHandle: 'sensor',
  };
  assert.equal(isValidConnection(connection, nodes, []), true);
});
