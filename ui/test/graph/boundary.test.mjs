import { test } from 'node:test';
import assert from 'node:assert/strict';
import { configToGraph } from '../../src/lib/graph/model.js';
import {
  projectScope,
  effectivePorts,
  resolveSource,
  resolveTargets,
  destinationSummary,
  endpointDisplayLabel,
} from '../../src/lib/graph/groups.js';
import { tempValue } from '../../src/lib/graph/liveValues.js';

function coolingFixture() {
  const config = {
    tick_interval_ms: 500,
    active_profile: 'default',
    profiles: {
      default: {
        sensors: { probe: { type: 'max', inputs: ['hwmon/chipA/temp2', 'hwmon/chipA/temp4'] } },
        curves: { airflow: { type: 'point', sensor: 'virtual/probe', points: [[30, 20], [70, 100]] } },
        assignments: { 'hwmon/chipA/pwm2': 'airflow', 'hwmon/chipA/pwm5': 'airflow' },
      },
    },
  };
  const inventory = {
    sensors: [
      { id: 'hwmon/chipA/temp2', label: 'VRM', kind: 'temp' },
      { id: 'hwmon/chipA/temp4', label: 'Case Air', kind: 'temp' },
    ],
    controls: [
      { id: 'hwmon/chipA/pwm2', label: 'Front intake' },
      { id: 'hwmon/chipA/pwm5', label: 'Side intake' },
    ],
  };
  const { nodes, edges } = configToGraph(config, inventory, 'default');
  const groups = [{ id: 'g1', name: 'Case cooling', position: { x: 0, y: 0 }, members: ['virtual:probe', 'curve:airflow'], inputs: [], outputs: [], parent: null }];
  return { nodes, edges, groups };
}

const snap = { sensors: { 'hwmon/chipA/temp2': 63.5, 'hwmon/chipA/temp4': 41.2 }, curves: {}, duties: {} };

test('an Inputs strip handle resolves to the outside sensor feeding that member input', () => {
  const { nodes, edges, groups } = coolingFixture();
  const view = projectScope(nodes, edges, groups, 'g1');
  const into = view.edges.filter((edge) => edge.source === 'port:in:g1');
  assert.equal(into.length, 2);
  const values = into.map((edge) => {
    const origin = resolveSource(edge.source, edge.sourceHandle, edges);
    return tempValue(snap, origin.node, origin.handle);
  });
  assert.deepEqual(values.sort(), [41.2, 63.5]);
});

test('a declared but unwired input resolves to nothing', () => {
  const { edges } = coolingFixture();
  assert.equal(resolveSource('port:in:g1', 'virtual:probe|in-2', edges), null);
});

test('a group node handle resolves to the member endpoint behind it', () => {
  const { edges } = coolingFixture();
  assert.deepEqual(resolveSource('group:g1', 'virtual:probe|out', edges), { node: 'virtual:probe', handle: 'out' });
  assert.deepEqual(resolveTargets('group:g1', 'virtual:probe|in-0', edges, []), [{ node: 'virtual:probe', handle: 'in-0' }]);
});

test('plain endpoints resolve to themselves', () => {
  const { edges, groups } = coolingFixture();
  assert.deepEqual(resolveSource('sensor:hwmon/chipA', 'hwmon/chipA/temp2', edges), { node: 'sensor:hwmon/chipA', handle: 'hwmon/chipA/temp2' });
  assert.deepEqual(resolveTargets('curve:airflow', 'sensor', edges, groups), [{ node: 'curve:airflow', handle: 'sensor' }]);
});

test('an Outputs strip handle resolves to every outside destination', () => {
  const { edges, groups } = coolingFixture();
  const targets = resolveTargets('port:out:g1', 'curve:airflow|out', edges, groups);
  assert.deepEqual(targets.map((target) => target.handle).sort(), ['hwmon/chipA/pwm2', 'hwmon/chipA/pwm5']);
});

test('effectivePorts lists every destination of a fanned-out output', () => {
  const { nodes, edges, groups } = coolingFixture();
  const { outputs } = effectivePorts(groups[0], nodes, edges);
  assert.equal(outputs.length, 1);
  assert.equal(outputs[0].destinations.length, 2);
  assert.deepEqual(outputs[0].outside, outputs[0].destinations[0]);
});

test('a declared output with no outside reader has no destinations', () => {
  const { nodes, edges, groups } = coolingFixture();
  const declared = [{ ...groups[0], outputs: [{ node: 'virtual:probe', handle: 'out', name: 'probe' }] }];
  const { outputs } = effectivePorts(declared[0], nodes, edges);
  const probe = outputs.find((port) => port.node === 'virtual:probe');
  assert.equal(probe.wired, false);
  assert.deepEqual(probe.destinations, []);
  assert.equal(probe.outside, null);
});

test('destinationSummary names one destination and counts fans for several', () => {
  const { nodes, edges, groups } = coolingFixture();
  const labelOf = (node, handle) => endpointDisplayLabel(nodes, node, handle);
  const [port] = effectivePorts(groups[0], nodes, edges).outputs;

  const many = destinationSummary(port.destinations, labelOf);
  assert.equal(many.text, '2 fans');
  assert.equal(many.title, 'chipA · Front intake, chipA · Side intake');

  assert.equal(destinationSummary(port.destinations, labelOf, 'Case duty').text, 'Case duty · 2 fans');
  assert.equal(destinationSummary(port.destinations.slice(0, 1), labelOf).text, 'chipA · Front intake');
  assert.deepEqual(destinationSummary([], labelOf), { text: 'spare', title: '' });
  assert.equal(destinationSummary([{ node: 'combine:blend', handle: 'in-0' }, { node: 'control:hwmon/chipA', handle: 'pwm1' }], () => 'x').text, '2 targets');
});
