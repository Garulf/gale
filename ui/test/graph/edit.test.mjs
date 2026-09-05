import { test } from 'node:test';
import assert from 'node:assert/strict';
import { CURVE_TYPES, nameInUse, renameNode, changeVirtualType, changeCurveType, applyPreset } from '../../src/lib/graph/edit.js';

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

function fixture() {
  const nodes = [
    { id: 'sensor:hwmon/x', type: 'deviceSensor', position: { x: 0, y: 0 }, data: { deviceSensor: { device: 'hwmon/x', rows: [] } } },
    { id: 'virtual:hot', type: 'virtual', position: { x: 0, y: 0 }, data: { virtual: { name: 'hot', config: { type: 'max', inputs: [] } } } },
    { id: 'curve:cpu', type: 'curve', position: { x: 0, y: 0 }, data: { curve: { id: 'cpu', config: { type: 'point', points: [[30, 20]], hysteresis: null, response: null } } } },
    { id: 'combine:both', type: 'combine', position: { x: 0, y: 0 }, data: { combine: { id: 'both', config: { type: 'mix', mode: 'max' } } } },
    { id: 'control:hwmon/x', type: 'deviceControl', position: { x: 0, y: 0 }, data: { deviceControl: { device: 'hwmon/x', rows: [] } } },
  ];
  const edges = [
    edge('sensor:hwmon/x', 'hwmon/x/temp1', 'virtual:hot', 'in-0', 'temp'),
    edge('sensor:hwmon/x', 'hwmon/x/temp2', 'virtual:hot', 'in-1', 'temp'),
    edge('virtual:hot', 'out', 'curve:cpu', 'sensor', 'temp'),
    edge('curve:cpu', 'out', 'combine:both', 'in-0', 'duty'),
    edge('combine:both', 'out', 'control:hwmon/x', 'hwmon/x/pwm1', 'duty'),
  ];
  return { nodes, edges };
}

test('CURVE_TYPES lists the six curve types', () => {
  assert.deepEqual(CURVE_TYPES, ['point', 'flat', 'mix', 'sync', 'trigger', 'target']);
});

test('nameInUse treats curve and combine ids as one namespace and ignores the node itself', () => {
  const { nodes } = fixture();
  assert.equal(nameInUse(nodes, 'curve', 'both', 'curve:cpu'), true);
  assert.equal(nameInUse(nodes, 'combine', 'cpu', 'combine:both'), true);
  assert.equal(nameInUse(nodes, 'curve', 'cpu', 'curve:cpu'), false);
  assert.equal(nameInUse(nodes, 'virtual', 'hot', 'virtual:hot'), false);
  assert.equal(nameInUse(nodes, 'virtual', 'hot', 'curve:cpu'), true);
  assert.equal(nameInUse(nodes, 'virtual', 'cpu', 'virtual:hot'), false);
});

test('renameNode rewrites the node id, its data and every touching edge', () => {
  const { nodes, edges } = fixture();
  const result = renameNode(nodes, edges, 'virtual:hot', 'gpu_hot');
  assert.equal(result.id, 'virtual:gpu_hot');
  const renamed = result.nodes.find((node) => node.id === 'virtual:gpu_hot');
  assert.equal(renamed.data.virtual.name, 'gpu_hot');
  assert.deepEqual(renamed.data.virtual.config, { type: 'max', inputs: [] });
  assert.equal(result.nodes.some((node) => node.id === 'virtual:hot'), false);
  assert.equal(result.edges.length, edges.length);
  assert.equal(result.edges.some((e) => e.source === 'virtual:hot' || e.target === 'virtual:hot'), false);
  const incoming = result.edges.filter((e) => e.target === 'virtual:gpu_hot').map((e) => e.targetHandle);
  assert.deepEqual(incoming, ['in-0', 'in-1']);
  const outgoing = result.edges.find((e) => e.source === 'virtual:gpu_hot');
  assert.equal(outgoing.target, 'curve:cpu');
  assert.equal(outgoing.id, 'virtual:gpu_hot:out->curve:cpu:sensor');
});

test('renameNode on a curve keeps the curve kind and updates data.curve.id', () => {
  const { nodes, edges } = fixture();
  const result = renameNode(nodes, edges, 'curve:cpu', 'cpu_fan');
  assert.equal(result.id, 'curve:cpu_fan');
  const renamed = result.nodes.find((node) => node.id === 'curve:cpu_fan');
  assert.equal(renamed.type, 'curve');
  assert.equal(renamed.data.curve.id, 'cpu_fan');
  assert.equal(result.edges.find((e) => e.target === 'combine:both').source, 'curve:cpu_fan');
  assert.equal(result.edges.find((e) => e.targetHandle === 'sensor').target, 'curve:cpu_fan');
});

test('changeVirtualType from a multi-input type to a single-input type keeps only in-0 as in', () => {
  const { nodes, edges } = fixture();
  const result = changeVirtualType(nodes, edges, 'virtual:hot', 'offset');
  assert.equal(result.id, 'virtual:hot');
  const node = result.nodes.find((n) => n.id === 'virtual:hot');
  assert.deepEqual(node.data.virtual.config, { type: 'offset', input: '', add: 0, scale: 1 });
  const incoming = result.edges.filter((e) => e.target === 'virtual:hot');
  assert.equal(incoming.length, 1);
  assert.equal(incoming[0].sourceHandle, 'hwmon/x/temp1');
  assert.equal(incoming[0].targetHandle, 'in');
  assert.equal(incoming[0].id, 'sensor:hwmon/x:hwmon/x/temp1->virtual:hot:in');
  assert.equal(result.edges.some((e) => e.source === 'virtual:hot'), true);
});

test('changeVirtualType back to a multi-input type maps in to in-0 and keeps sibling types intact', () => {
  const { nodes, edges } = fixture();
  const single = changeVirtualType(nodes, edges, 'virtual:hot', 'delta');
  const multi = changeVirtualType(single.nodes, single.edges, 'virtual:hot', 'mean');
  const incoming = multi.edges.filter((e) => e.target === 'virtual:hot');
  assert.deepEqual(incoming.map((e) => e.targetHandle), ['in-0']);
  assert.deepEqual(multi.nodes.find((n) => n.id === 'virtual:hot').data.virtual.config, { type: 'mean', inputs: [], window_s: null });
  const same = changeVirtualType(multi.nodes, multi.edges, 'virtual:hot', 'min');
  assert.deepEqual(same.edges.filter((e) => e.target === 'virtual:hot').map((e) => e.targetHandle), ['in-0']);
});

test('changeVirtualType to the same type returns the inputs untouched', () => {
  const { nodes, edges } = fixture();
  const result = changeVirtualType(nodes, edges, 'virtual:hot', 'max');
  assert.equal(result.nodes, nodes);
  assert.equal(result.edges, edges);
});

test('changeVirtualType to webhook drops every incoming edge and keeps the output edge', () => {
  const { nodes, edges } = fixture();
  const result = changeVirtualType(nodes, edges, 'virtual:hot', 'webhook');
  const node = result.nodes.find((n) => n.id === 'virtual:hot');
  assert.deepEqual(node.data.virtual.config, { type: 'webhook', token: '', timeout_s: null });
  assert.equal(result.edges.filter((e) => e.target === 'virtual:hot').length, 0);
  assert.equal(result.edges.some((e) => e.source === 'virtual:hot' && e.target === 'curve:cpu'), true);
  assert.equal(result.edges.length, edges.length - 2);
});

test('changeVirtualType from webhook to max yields a node with no inputs and no incoming edges', () => {
  const { nodes, edges } = fixture();
  const toWebhook = changeVirtualType(nodes, edges, 'virtual:hot', 'webhook');
  const result = changeVirtualType(toWebhook.nodes, toWebhook.edges, 'virtual:hot', 'max');
  const node = result.nodes.find((n) => n.id === 'virtual:hot');
  assert.deepEqual(node.data.virtual.config, { type: 'max', inputs: [] });
  assert.equal(result.edges.filter((e) => e.target === 'virtual:hot').length, 0);
});

test('changeCurveType between sensor-driven types keeps the sensor edge and the output edge', () => {
  const { nodes, edges } = fixture();
  const result = changeCurveType(nodes, edges, 'curve:cpu', 'trigger');
  assert.equal(result.id, 'curve:cpu');
  const node = result.nodes.find((n) => n.id === 'curve:cpu');
  assert.equal(node.type, 'curve');
  assert.equal(node.data.curve.config.type, 'trigger');
  assert.equal(result.edges.length, edges.length);
});

test('changeCurveType to flat drops the sensor edge but keeps the output edge', () => {
  const { nodes, edges } = fixture();
  const result = changeCurveType(nodes, edges, 'curve:cpu', 'flat');
  assert.equal(result.edges.some((e) => e.target === 'curve:cpu'), false);
  assert.equal(result.edges.some((e) => e.source === 'curve:cpu' && e.target === 'combine:both'), true);
});

test('changeCurveType from a curve to mix moves the node to the combine kind and re-points its output', () => {
  const { nodes, edges } = fixture();
  const result = changeCurveType(nodes, edges, 'curve:cpu', 'mix');
  assert.equal(result.id, 'combine:cpu');
  const node = result.nodes.find((n) => n.id === 'combine:cpu');
  assert.equal(node.type, 'combine');
  assert.deepEqual(node.data, { combine: { id: 'cpu', config: { type: 'mix', sources: [], mode: 'max' } } });
  assert.equal(result.nodes.some((n) => n.id === 'curve:cpu'), false);
  assert.equal(result.edges.some((e) => e.target === 'combine:cpu'), false);
  const out = result.edges.find((e) => e.source === 'combine:cpu');
  assert.equal(out.target, 'combine:both');
  assert.equal(out.id, 'combine:cpu:out->combine:both:in-0');
});

test('changeCurveType between mix and sync remaps the first input handle both ways', () => {
  const { nodes, edges } = fixture();
  const sync = changeCurveType(nodes, edges, 'combine:both', 'sync');
  assert.equal(sync.id, 'combine:both');
  assert.deepEqual(sync.edges.filter((e) => e.target === 'combine:both').map((e) => e.targetHandle), ['in']);
  assert.equal(sync.edges.some((e) => e.source === 'combine:both' && e.targetHandle === 'hwmon/x/pwm1'), true);
  const mix = changeCurveType(sync.nodes, sync.edges, 'combine:both', 'mix');
  assert.deepEqual(mix.edges.filter((e) => e.target === 'combine:both').map((e) => e.targetHandle), ['in-0']);
});

test('changeCurveType from combine to a point curve moves back to the curve kind with no inputs', () => {
  const { nodes, edges } = fixture();
  const result = changeCurveType(nodes, edges, 'combine:both', 'point');
  assert.equal(result.id, 'curve:both');
  assert.equal(result.nodes.find((n) => n.id === 'curve:both').type, 'curve');
  assert.equal(result.edges.some((e) => e.target === 'curve:both'), false);
  assert.equal(result.edges.find((e) => e.source === 'curve:both').target, 'control:hwmon/x');
});

test('applyPreset replaces the curve config in place when the type matches', () => {
  const { nodes, edges } = fixture();
  const preset = { type: 'point', points: [[30, 20], [50, 30], [85, 100]], hysteresis: null, response: null };
  const result = applyPreset(nodes, edges, 'curve:cpu', preset);
  assert.equal(result.id, 'curve:cpu');
  assert.deepEqual(result.nodes.find((node) => node.id === 'curve:cpu').data.curve.config, preset);
  assert.deepEqual(result.edges, edges);
  assert.notEqual(result.nodes.find((node) => node.id === 'curve:cpu').data.curve.config.points, preset.points);
});

test('applyPreset across types goes through the retype path so edges are remapped', () => {
  const { nodes, edges } = fixture();
  const preset = { type: 'flat', duty: 35 };
  const result = applyPreset(nodes, edges, 'curve:cpu', preset);
  const node = result.nodes.find((candidate) => candidate.id === 'curve:cpu');
  assert.equal(node.type, 'curve');
  assert.deepEqual(node.data.curve.config, preset);
  assert.equal(result.edges.some((edge) => edge.target === 'curve:cpu'), false);
  assert.equal(result.edges.some((edge) => edge.source === 'curve:cpu'), true);
});
