import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  attributeWarning,
  unwiredRequiredInputs,
  kindMismatchedEdges,
  collectNodeWarnings,
} from '../../src/lib/graph/nodeWarnings.js';

const SENSOR_NODE = {
  id: 'sensor:hwmon/nct6798',
  type: 'deviceSensor',
  data: {
    deviceSensor: {
      device: 'hwmon/nct6798',
      rows: [
        { handle: 'hwmon/nct6798/temp1', label: 'CPUTIN', kind: 'temp' },
        { handle: 'hwmon/nct6798/temp10', label: 'AUX', kind: 'temp' },
        { handle: 'hwmon/nct6798/fan1', label: 'fan1', kind: 'rpm' },
        { handle: 'hwmon/nct6798/pwm1', label: 'pwm1', kind: 'duty' },
      ],
    },
  },
};

function curve(id, type) {
  return { id: `curve:${id}`, type: 'curve', data: { curve: { id, config: { type } } } };
}

function combine(id, type) {
  return { id: `combine:${id}`, type: 'combine', data: { combine: { id, config: { type } } } };
}

function virtual(name, config) {
  return { id: `virtual:${name}`, type: 'virtual', data: { virtual: { name, config } } };
}

function edge(source, sourceHandle, target, targetHandle, kind = 'temp') {
  return { id: `${source}:${sourceHandle}->${target}:${targetHandle}`, source, sourceHandle, target, targetHandle, data: { kind } };
}

test('a hardware-not-found warning is attributed to the device node owning the row', () => {
  const nodes = [SENSOR_NODE, curve('cpu', 'point')];
  const edges = [edge(SENSOR_NODE.id, 'hwmon/nct6798/temp1', 'curve:cpu', 'sensor')];
  const attributed = attributeWarning('referenced sensor not found on hardware: hwmon/nct6798/temp1', nodes, edges);
  assert.deepEqual(attributed.sort(), ['curve:cpu', 'sensor:hwmon/nct6798']);
});

test('id matching respects boundaries so temp1 does not match temp10', () => {
  const nodes = [SENSOR_NODE, curve('a', 'point'), curve('b', 'point')];
  const edges = [
    edge(SENSOR_NODE.id, 'hwmon/nct6798/temp1', 'curve:a', 'sensor'),
    edge(SENSOR_NODE.id, 'hwmon/nct6798/temp10', 'curve:b', 'sensor'),
  ];
  const attributed = attributeWarning('referenced sensor not found on hardware: hwmon/nct6798/temp10', nodes, edges);
  assert.deepEqual(attributed.sort(), ['curve:b', 'sensor:hwmon/nct6798']);
});

test('a sensor id absent from the inventory is attributed via the edges that consume it', () => {
  const nodes = [SENSOR_NODE, curve('cpu', 'point'), virtual('hot', { type: 'max', inputs: [] })];
  const edges = [
    edge(SENSOR_NODE.id, 'hwmon/nct6798/temp9', 'curve:cpu', 'sensor'),
    edge(SENSOR_NODE.id, 'hwmon/nct6798/temp9', 'virtual:hot', 'in-0'),
  ];
  const attributed = attributeWarning('referenced sensor not found on hardware: hwmon/nct6798/temp9', nodes, edges);
  assert.deepEqual(attributed.sort(), ['curve:cpu', 'sensor:hwmon/nct6798', 'virtual:hot']);
});

test('a virtual sensor window warning is attributed to the virtual node by quoted name', () => {
  const nodes = [virtual('smooth', { type: 'mean', inputs: [], window_s: 0.1 }), virtual('smoother', { type: 'mean', inputs: [] })];
  const message = "virtual sensor 'smooth' has window_s shorter than the tick interval and will not smooth or measure correctly";
  assert.deepEqual(attributeWarning(message, nodes, []), ['virtual:smooth']);
});

test('a warning with no matching id is not attributed to any node', () => {
  const nodes = [SENSOR_NODE, curve('cpu', 'point')];
  assert.deepEqual(attributeWarning('PawnIO driver not installed, see https://pawnio.eu', nodes, []), []);
  assert.deepEqual(attributeWarning('referenced sensor not found on hardware: hwmon/other/temp1', nodes, []), []);
});

test('point, trigger and target curves with an unwired sensor handle are flagged', () => {
  const nodes = [curve('p', 'point'), curve('t', 'trigger'), curve('g', 'target')];
  const flagged = unwiredRequiredInputs(nodes, []).map((entry) => entry.nodeId);
  assert.deepEqual(flagged, ['curve:p', 'curve:t', 'curve:g']);
});

test('a flat curve is never flagged and a wired point curve is not flagged', () => {
  const nodes = [SENSOR_NODE, curve('flat', 'flat'), curve('cpu', 'point')];
  const edges = [edge(SENSOR_NODE.id, 'hwmon/nct6798/temp1', 'curve:cpu', 'sensor')];
  assert.deepEqual(unwiredRequiredInputs(nodes, edges), []);
});

test('a mix combine with zero connected inputs is flagged, one with an input is not', () => {
  const nodes = [curve('a', 'flat'), combine('empty', 'mix'), combine('fed', 'mix')];
  const edges = [edge('curve:a', 'out', 'combine:fed', 'in-0', 'duty')];
  const flagged = unwiredRequiredInputs(nodes, edges).map((entry) => entry.nodeId);
  assert.deepEqual(flagged, ['combine:empty']);
});

test('a sync combine with nothing wired into "in" is flagged', () => {
  const nodes = [curve('a', 'flat'), combine('s', 'sync'), combine('ok', 'sync')];
  const edges = [edge('curve:a', 'out', 'combine:ok', 'in', 'duty')];
  const flagged = unwiredRequiredInputs(nodes, edges).map((entry) => entry.nodeId);
  assert.deepEqual(flagged, ['combine:s']);
});

test('a virtual sensor with no inputs is flagged, a wired one is not', () => {
  const nodes = [SENSOR_NODE, virtual('empty', { type: 'max', inputs: [] }), virtual('fed', { type: 'offset', input: '' })];
  const edges = [edge(SENSOR_NODE.id, 'hwmon/nct6798/temp1', 'virtual:fed', 'in')];
  const flagged = unwiredRequiredInputs(nodes, edges).map((entry) => entry.nodeId);
  assert.deepEqual(flagged, ['virtual:empty']);
});

test('a webhook virtual sensor is never flagged for having no inputs', () => {
  const nodes = [
    virtual('hook', { type: 'webhook', token: '', timeout_s: null }),
    virtual('empty', { type: 'max', inputs: [] }),
  ];
  const flagged = unwiredRequiredInputs(nodes, []).map((e) => e.nodeId);
  assert.deepEqual(flagged, ['virtual:empty']);
});

test('an rpm or duty device row feeding a temperature input is pre-existing bad wiring', () => {
  const nodes = [SENSOR_NODE, curve('cpu', 'point'), virtual('hot', { type: 'max', inputs: [] })];
  const edges = [
    edge(SENSOR_NODE.id, 'hwmon/nct6798/fan1', 'curve:cpu', 'sensor'),
    edge(SENSOR_NODE.id, 'hwmon/nct6798/pwm1', 'virtual:hot', 'in-0'),
  ];
  const flagged = kindMismatchedEdges(nodes, edges);
  assert.deepEqual(flagged.map((entry) => entry.nodeId), ['curve:cpu', 'virtual:hot']);
  assert.match(flagged[0].message, /fan1/);
  assert.match(flagged[0].message, /rpm/);
  assert.match(flagged[0].message, /cannot drive a curve/);
});

test('a power sensor row feeding a curve or virtual input is not flagged', () => {
  const powerSensor = {
    id: 'sensor:hwmon/power',
    type: 'deviceSensor',
    data: {
      deviceSensor: {
        device: 'hwmon/power',
        rows: [{ handle: 'hwmon/power/power1', label: 'Package power', kind: 'power' }],
      },
    },
  };
  const nodes = [powerSensor, curve('cpu', 'point')];
  const edges = [edge(powerSensor.id, 'hwmon/power/power1', 'curve:cpu', 'sensor')];
  assert.deepEqual(kindMismatchedEdges(nodes, edges), []);
});

test('temp-to-temp edges, virtual sources, rows of unknown kind and duty edges are not flagged', () => {
  const nodes = [SENSOR_NODE, curve('cpu', 'point'), virtual('hot', { type: 'max', inputs: [] }), combine('m', 'mix')];
  const edges = [
    edge(SENSOR_NODE.id, 'hwmon/nct6798/temp1', 'curve:cpu', 'sensor'),
    edge(SENSOR_NODE.id, 'hwmon/nct6798/temp99', 'virtual:hot', 'in-0'),
    edge('virtual:hot', 'out', 'curve:cpu', 'sensor'),
    edge('curve:cpu', 'out', 'combine:m', 'in-0', 'duty'),
  ];
  assert.deepEqual(kindMismatchedEdges(nodes, edges), []);
});

test('collectNodeWarnings merges all three sources per node and drops unattributable messages', () => {
  const nodes = [SENSOR_NODE, curve('cpu', 'point'), curve('lonely', 'point'), curve('flat', 'flat')];
  const edges = [edge(SENSOR_NODE.id, 'hwmon/nct6798/fan1', 'curve:cpu', 'sensor')];
  const daemon = [
    'PawnIO driver not installed',
    'referenced sensor not found on hardware: hwmon/nct6798/fan1',
    'referenced sensor not found on hardware: hwmon/nct6798/fan1',
  ];
  const collected = collectNodeWarnings(nodes, edges, daemon);
  assert.deepEqual(Object.keys(collected).sort(), ['curve:cpu', 'curve:lonely', 'sensor:hwmon/nct6798']);
  assert.equal(collected['curve:cpu'].length, 2);
  assert.equal(collected['sensor:hwmon/nct6798'].length, 1);
  assert.equal(collected['curve:lonely'].length, 1);
  assert.equal(collectNodeWarnings(nodes, edges, undefined)['curve:flat'], undefined);
});
