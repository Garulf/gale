import { test } from 'node:test';
import assert from 'node:assert/strict';
import { configToGraph, graphToConfig, adoptSavedTokens } from '../../src/lib/graph/model.js';

function fixtureAConfig() {
  return {
    tick_interval_ms: 1000,
    active_profile: 'default',
    api: { bind: '127.0.0.1:5250' },
    hardware: { corsair: { on_release: 'keep_last' } },
    profiles: {
      default: {
        sensors: {
          cpu_hot: { type: 'max', inputs: ['hwmon/nct6798/temp1', 'nvidia/0/temp'] },
        },
        curves: {
          cpu: { type: 'point', sensor: 'hwmon/nct6798/temp1', points: [[30.0, 20.0], [70.0, 100.0]] },
          case: { type: 'point', sensor: 'virtual/cpu_hot', points: [[35.0, 25.0], [75.0, 100.0]] },
        },
        assignments: {},
      },
    },
  };
}

function fixtureAInventory() {
  return {
    sensors: [
      { id: 'hwmon/nct6798/temp1', label: 'CPU Package', kind: 'temp' },
      { id: 'nvidia/0/temp', label: 'GPU Temperature', kind: 'temp' },
    ],
    controls: [],
    virtual: [{ id: 'virtual/cpu_hot', type: 'max', inputs: ['hwmon/nct6798/temp1', 'nvidia/0/temp'] }],
  };
}

function fixtureBProfile() {
  return {
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
  };
}

function fixtureBConfig() {
  return {
    tick_interval_ms: 1000,
    active_profile: 'default',
    profiles: { default: fixtureBProfile() },
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

test('configToGraph on fixture A produces node ids matching the example config device set', () => {
  const { nodes } = configToGraph(fixtureAConfig(), fixtureAInventory(), 'default');
  const ids = nodes.map((node) => node.id).sort();
  assert.deepEqual(ids, [
    'curve:case',
    'curve:cpu',
    'sensor:hwmon/nct6798',
    'sensor:nvidia/0',
    'virtual:cpu_hot',
  ]);
});

test('graphToConfig on fixture A unmodified graph deep-equals the original profile', () => {
  const config = fixtureAConfig();
  const { nodes, edges } = configToGraph(config, fixtureAInventory(), 'default');
  const result = graphToConfig(nodes, edges, config, 'default');
  const original = fixtureAConfig().profiles.default;
  assert.deepEqual(result.profiles.default.curves, original.curves);
  assert.deepEqual(result.profiles.default.sensors, original.sensors);
  assert.deepEqual(result.profiles.default.assignments, original.assignments);
});

test('fixture B full round trip deep-equals the input profile', () => {
  const config = fixtureBConfig();
  const { nodes, edges } = configToGraph(config, fixtureBInventory(), 'default');
  const result = graphToConfig(nodes, edges, config, 'default');
  assert.deepEqual(result.profiles.default, fixtureBProfile());
});

test('fixture B mix node has two duty input edges and point node has one temp input edge', () => {
  const config = fixtureBConfig();
  const { edges } = configToGraph(config, fixtureBInventory(), 'default');

  const mixEdges = edges.filter((edge) => edge.target === 'combine:blend');
  assert.equal(mixEdges.length, 2);
  for (const edge of mixEdges) {
    assert.equal(edge.data.kind, 'duty');
  }

  const pointEdges = edges.filter((edge) => edge.target === 'curve:cpu');
  assert.equal(pointEdges.length, 1);
  assert.equal(pointEdges[0].data.kind, 'temp');
});

test('graphToConfig_drops_wiring_field_when_handle_is_unconnected', () => {
  const config = fixtureBConfig();
  const { nodes, edges } = configToGraph(config, fixtureBInventory(), 'default');
  const filteredEdges = edges.filter((edge) => !(edge.target === 'curve:cpu' && edge.targetHandle === 'sensor'));

  const result = graphToConfig(nodes, filteredEdges, config, 'default');
  assert.equal(result.profiles.default.curves.cpu.sensor, '');
});

test('configToGraph_falls_back_to_deterministic_position_when_layout_is_empty', () => {
  const config = fixtureAConfig();
  const { nodes } = configToGraph(config, fixtureAInventory(), 'default');
  for (const node of nodes) {
    assert.ok([40, 340, 590, 830].includes(node.position.x), `unexpected x ${node.position.x} for ${node.id}`);
    assert.equal(typeof node.position.y, 'number');
  }
});

test('graphToConfig_preserves_position_of_hidden_nodes', () => {
  const config = fixtureBConfig();
  const { nodes, edges } = configToGraph(config, fixtureBInventory(), 'default');
  const hiddenNodes = nodes.map((node) =>
    node.id === 'sensor:hwmon/chipA' ? { ...node, hidden: true, position: { x: 999, y: 111 } } : node,
  );

  const result = graphToConfig(hiddenNodes, edges, config, 'default');
  assert.deepEqual(result.ui.graph.default['sensor:hwmon/chipA'], [999, 111]);
  assert.ok(result.ui.hidden.default.includes('sensor:hwmon/chipA'));
});

test('configToGraph wires a single-input max virtual sensor to in-0, not in', () => {
  const config = {
    tick_interval_ms: 1000,
    active_profile: 'default',
    profiles: {
      default: {
        sensors: {
          hot: { type: 'max', inputs: ['hwmon/chipA/temp1'] },
        },
        curves: {},
        assignments: {},
      },
    },
  };
  const inventory = {
    sensors: [{ id: 'hwmon/chipA/temp1', label: 'CPU', kind: 'temp' }],
    controls: [],
  };

  const { edges } = configToGraph(config, inventory, 'default');
  const incoming = edges.filter((edge) => edge.target === 'virtual:hot');
  assert.equal(incoming.length, 1);
  assert.equal(incoming[0].targetHandle, 'in-0');
});

test('configToGraph still wires single-input offset and delta sensors to in', () => {
  const config = {
    tick_interval_ms: 1000,
    active_profile: 'default',
    profiles: {
      default: {
        sensors: {
          off: { type: 'offset', input: 'hwmon/chipA/temp1', add: 0, scale: 1 },
          delt: { type: 'delta', input: 'hwmon/chipA/temp1', window_s: 30 },
        },
        curves: {},
        assignments: {},
      },
    },
  };
  const inventory = {
    sensors: [{ id: 'hwmon/chipA/temp1', label: 'CPU', kind: 'temp' }],
    controls: [],
  };

  const { edges } = configToGraph(config, inventory, 'default');
  const offEdge = edges.find((edge) => edge.target === 'virtual:off');
  const deltEdge = edges.find((edge) => edge.target === 'virtual:delt');
  assert.equal(offEdge.targetHandle, 'in');
  assert.equal(deltEdge.targetHandle, 'in');
});

test('graphToConfig_only_persists_hidden_for_device_nodes', () => {
  const config = fixtureBConfig();
  const { nodes, edges } = configToGraph(config, fixtureBInventory(), 'default');
  const withHiddenVirtual = nodes.map((node) => (node.id === 'virtual:hot' ? { ...node, hidden: true } : node));

  const result = graphToConfig(withHiddenVirtual, edges, config, 'default');
  assert.equal(result.ui.hidden.default.includes('virtual:hot'), false);
});

function wideDeviceConfig() {
  return {
    tick_interval_ms: 1000,
    active_profile: 'default',
    profiles: {
      default: {
        sensors: {},
        curves: {
          late: { type: 'point', sensor: 'hwmon/chipA/temp5', points: [[30, 20], [70, 100]] },
        },
        assignments: { 'hwmon/chipA/pwm5': 'late' },
      },
    },
  };
}

function wideDeviceInventory() {
  return {
    sensors: [
      { id: 'hwmon/chipA/temp1', label: 'CPU', kind: 'temp' },
      { id: 'hwmon/chipA/temp2', label: 'SYS', kind: 'temp' },
      { id: 'hwmon/chipA/temp3', label: 'AUX', kind: 'temp' },
      { id: 'hwmon/chipA/temp4', label: 'VRM', kind: 'temp' },
      { id: 'hwmon/chipA/temp5', label: 'PCH', kind: 'temp' },
      { id: 'hwmon/chipA/fan1', label: 'CPU fan speed', kind: 'rpm' },
    ],
    controls: [1, 2, 3, 4, 5].map((n) => ({ id: `hwmon/chipA/pwm${n}`, label: `pwm${n}` })),
  };
}

function webhookConfig() {
  return {
    tick_interval_ms: 1000,
    active_profile: 'default',
    profiles: {
      default: {
        sensors: {
          remote: { type: 'webhook', token: 'abc', timeout_s: 60 },
          forever: { type: 'webhook', token: 'def' },
        },
        curves: {
          cpu: { type: 'point', sensor: 'virtual/remote', points: [[30, 20], [70, 100]] },
        },
        assignments: {},
      },
    },
  };
}

function webhookInventory() {
  return { sensors: [], controls: [] };
}

test('configToGraph builds webhook nodes with no incoming edges', () => {
  const { nodes, edges } = configToGraph(webhookConfig(), webhookInventory(), 'default');
  const ids = nodes.map((node) => node.id);
  assert.ok(ids.includes('virtual:remote'));
  assert.ok(ids.includes('virtual:forever'));
  assert.equal(edges.filter((e) => e.target === 'virtual:remote').length, 0);
  assert.equal(edges.length, 1);
  assert.equal(edges[0].id, 'virtual:remote:out->curve:cpu:sensor');
});

test('graphToConfig round-trips webhook sensors without adding an input field', () => {
  const config = webhookConfig();
  const { nodes, edges } = configToGraph(config, webhookInventory(), 'default');
  const result = graphToConfig(nodes, edges, config, 'default');
  assert.deepEqual(result.profiles.default.sensors, webhookConfig().profiles.default.sensors);
  assert.ok(!('input' in result.profiles.default.sensors.remote));
  assert.ok(!('inputs' in result.profiles.default.sensors.remote));
  assert.ok(!('input' in result.profiles.default.sensors.forever));
  assert.ok(!('inputs' in result.profiles.default.sensors.forever));
});

test('adoptSavedTokens fills empty webhook tokens from the saved profile and leaves everything else alone', () => {
  const nodes = [
    { id: 'virtual:remote', type: 'virtual', data: { virtual: { name: 'remote', config: { type: 'webhook', token: '', timeout_s: null } } } },
    { id: 'virtual:forever', type: 'virtual', data: { virtual: { name: 'forever', config: { type: 'webhook', token: 'keep', timeout_s: null } } } },
    { id: 'virtual:hot', type: 'virtual', data: { virtual: { name: 'hot', config: { type: 'max', inputs: [] } } } },
  ];
  const savedProfile = {
    sensors: {
      remote: { type: 'webhook', token: 'newtoken' },
      forever: { type: 'webhook', token: 'other' },
      hot: { type: 'max', inputs: [] },
    },
  };
  const result = adoptSavedTokens(nodes, savedProfile);
  assert.equal(result.find((n) => n.id === 'virtual:remote').data.virtual.config.token, 'newtoken');
  assert.equal(result.find((n) => n.id === 'virtual:forever').data.virtual.config.token, 'keep');
  assert.equal(result.find((n) => n.id === 'virtual:hot'), nodes[2]);

  const withMissing = [
    { id: 'virtual:ghost', type: 'virtual', data: { virtual: { name: 'ghost', config: { type: 'webhook', token: '', timeout_s: null } } } },
  ];
  const missingResult = adoptSavedTokens(withMissing, { sensors: {} });
  assert.equal(missingResult[0], withMissing[0]);
});

test('configToGraph marks device rows wired by their edges regardless of row position', () => {
  const { nodes } = configToGraph(wideDeviceConfig(), wideDeviceInventory(), 'default');
  const sensorRows = nodes.find((node) => node.id === 'sensor:hwmon/chipA').data.deviceSensor.rows;
  const controlRows = nodes.find((node) => node.id === 'control:hwmon/chipA').data.deviceControl.rows;

  assert.deepEqual(
    sensorRows.map((row) => row.wired),
    [false, false, false, false, true, false],
  );
  assert.equal(sensorRows[4].handle, 'hwmon/chipA/temp5');
  assert.deepEqual(
    controlRows.map((row) => row.wired),
    [false, false, false, false, true],
  );
  assert.equal(controlRows[4].handle, 'hwmon/chipA/pwm5');
});

test('configToGraph preserves each sensor row kind from the inventory', () => {
  const { nodes } = configToGraph(wideDeviceConfig(), wideDeviceInventory(), 'default');
  const rows = nodes.find((node) => node.id === 'sensor:hwmon/chipA').data.deviceSensor.rows;
  assert.equal(rows.find((row) => row.handle === 'hwmon/chipA/temp1').kind, 'temp');
  assert.equal(rows.find((row) => row.handle === 'hwmon/chipA/fan1').kind, 'rpm');
});
