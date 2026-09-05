import { test } from 'node:test';
import assert from 'node:assert/strict';
import { buildChains, chainDevices, chainMatchesFilter } from '../../src/lib/graph/chains.js';

const nodes = [
  { id: 'sensor:hwmon/x', type: 'deviceSensor', data: { deviceSensor: { device: 'hwmon/x', rows: [] } } },
  { id: 'sensor:nvidia/0', type: 'deviceSensor', data: { deviceSensor: { device: 'nvidia/0', rows: [] } } },
  { id: 'control:hwmon/x', type: 'deviceControl', data: { deviceControl: { device: 'hwmon/x', rows: [] } } },
  { id: 'virtual:hot', type: 'virtual', data: { virtual: { name: 'hot', config: { type: 'max' } } } },
  { id: 'curve:cpu', type: 'curve', data: { curve: { id: 'cpu', config: { type: 'point' } } } },
  { id: 'curve:orphan', type: 'curve', data: { curve: { id: 'orphan', config: { type: 'flat' } } } },
];

const edges = [
  { source: 'sensor:hwmon/x', sourceHandle: 'hwmon/x/temp1', target: 'virtual:hot', targetHandle: 'in-0' },
  { source: 'sensor:nvidia/0', sourceHandle: 'nvidia/0/temp', target: 'virtual:hot', targetHandle: 'in-1' },
  { source: 'virtual:hot', sourceHandle: 'out', target: 'curve:cpu', targetHandle: 'sensor' },
  { source: 'curve:cpu', sourceHandle: 'out', target: 'control:hwmon/x', targetHandle: 'hwmon/x/pwm1' },
  { source: 'curve:cpu', sourceHandle: 'out', target: 'control:hwmon/x', targetHandle: 'hwmon/x/pwm2' },
];

test('buildChains walks sensors through virtual and curve nodes to every driven control', () => {
  const chains = buildChains(nodes, edges);
  assert.equal(chains.length, 2);
  assert.deepEqual(
    chains[0].steps.map((step) => `${step.nodeId}|${step.handle}`),
    [
      'sensor:hwmon/x|hwmon/x/temp1',
      'sensor:nvidia/0|nvidia/0/temp',
      'virtual:hot|out',
      'curve:cpu|out',
      'control:hwmon/x|hwmon/x/pwm1',
      'control:hwmon/x|hwmon/x/pwm2',
    ]
  );
  assert.equal(chains[1].id, 'curve:orphan');
  assert.equal(chains[1].unassigned, true);
});

test('chainDevices and chainMatchesFilter use the vendor prefix of touched devices', () => {
  const chains = buildChains(nodes, edges);
  assert.deepEqual(chainDevices(chains, nodes), ['hwmon', 'nvidia']);
  assert.equal(chainMatchesFilter(chains[0], nodes, 'nvidia'), true);
  assert.equal(chainMatchesFilter(chains[1], nodes, 'nvidia'), false);
  assert.equal(chainMatchesFilter(chains[0], nodes, 'manual', ['hwmon/x/pwm2']), true);
  assert.equal(chainMatchesFilter(chains[0], nodes, 'manual', []), false);
});

test('buildChains lists a sensor feeding a combine through two branches only once', () => {
  const extra = [
    ...nodes,
    { id: 'curve:case', type: 'curve', data: { curve: { id: 'case', config: { type: 'point' } } } },
    { id: 'combine:front', type: 'combine', data: { combine: { id: 'front', config: { type: 'mix', mode: 'max' } } } },
  ];
  const wired = [
    ...edges,
    { source: 'sensor:hwmon/x', sourceHandle: 'hwmon/x/temp1', target: 'curve:case', targetHandle: 'sensor' },
    { source: 'curve:cpu', sourceHandle: 'out', target: 'combine:front', targetHandle: 'in-0' },
    { source: 'curve:case', sourceHandle: 'out', target: 'combine:front', targetHandle: 'in-1' },
    { source: 'combine:front', sourceHandle: 'out', target: 'control:hwmon/x', targetHandle: 'hwmon/x/pwm3' },
  ];
  const front = buildChains(extra, wired).find((chain) => chain.id === 'combine:front');
  const keys = front.steps.map((step) => `${step.nodeId}|${step.handle}`);
  assert.equal(new Set(keys).size, keys.length);
  assert.ok(keys.includes('sensor:hwmon/x|hwmon/x/temp1'));
});

test('buildChains folds an upstream virtual sensor into its unassigned curve instead of a second chain', () => {
  const spareNodes = [
    { id: 'sensor:hwmon/x', type: 'deviceSensor', data: { deviceSensor: { device: 'hwmon/x', rows: [] } } },
    { id: 'virtual:hot', type: 'virtual', data: { virtual: { name: 'hot', config: { type: 'max' } } } },
    { id: 'curve:spare', type: 'curve', data: { curve: { id: 'spare', config: { type: 'point' } } } },
  ];
  const spareEdges = [
    { source: 'sensor:hwmon/x', sourceHandle: 'hwmon/x/temp1', target: 'virtual:hot', targetHandle: 'in-0' },
    { source: 'virtual:hot', sourceHandle: 'out', target: 'curve:spare', targetHandle: 'sensor' },
  ];
  const chains = buildChains(spareNodes, spareEdges);
  assert.equal(chains.length, 1);
  assert.equal(chains[0].id, 'curve:spare');
  assert.equal(chains[0].unassigned, true);
  assert.deepEqual(
    chains[0].steps.map((step) => step.nodeId),
    ['sensor:hwmon/x', 'virtual:hot', 'curve:spare']
  );
});
