import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  groupNodeId, isGroupNodeId, groupIdOf,
  portNodeId, isPortNodeId, parsePortNodeId,
  groupHandleId, parseGroupHandleId,
  groupOf, nextGroupId, createGroup, ungroup, setMembership, renameMember, dropMember, pruneGroups,
} from '../../src/lib/graph/groups.js';

const at = { x: 10, y: 20 };

test('id helpers round-trip node ids that themselves contain colons and slashes', () => {
  assert.equal(groupNodeId('g3'), 'group:g3');
  assert.ok(isGroupNodeId('group:g3'));
  assert.equal(groupIdOf('group:g3'), 'g3');

  const port = portNodeId('sensor:corsair/commander-pro-1', 'corsair/commander-pro-1/temp1', 'in');
  assert.ok(isPortNodeId(port));
  assert.deepEqual(parsePortNodeId(port), { direction: 'in', nodeId: 'sensor:corsair/commander-pro-1', handle: 'corsair/commander-pro-1/temp1' });

  const handle = groupHandleId('combine:radiator_zone', 'out');
  assert.equal(handle, 'combine:radiator_zone|out');
  assert.deepEqual(parseGroupHandleId(handle), { nodeId: 'combine:radiator_zone', handle: 'out' });
});

test('createGroup takes members away from any other group and numbers ids without reuse gaps', () => {
  const first = createGroup([], ['curve:a', 'curve:b'], at);
  assert.equal(first.id, 'g1');
  assert.deepEqual(first.groups, [{ id: 'g1', name: 'Group 1', position: at, members: ['curve:a', 'curve:b'], parent: null }]);

  const second = createGroup(first.groups, ['curve:b', 'curve:c'], at, 'Case');
  assert.equal(second.id, 'g2');
  assert.deepEqual(second.groups.find((g) => g.id === 'g1').members, ['curve:a']);
  assert.deepEqual(second.groups.find((g) => g.id === 'g2'), { id: 'g2', name: 'Case', position: at, members: ['curve:b', 'curve:c'], parent: null });
  assert.equal(nextGroupId(second.groups), 'g3');
});

test('createGroup ignores group and port ids and returns null when nothing usable remains', () => {
  const result = createGroup([], ['group:g1', 'port:in:sensor:x:x/temp1'], at);
  assert.equal(result.id, null);
  assert.deepEqual(result.groups, []);
});

test('setMembership moves a node between groups, removes empty groups, and null means no group', () => {
  const { groups } = createGroup([], ['curve:a', 'curve:b'], at);
  const moved = setMembership(createGroup(groups, ['curve:c'], at).groups, 'curve:c', 'g1');
  assert.equal(moved.length, 1);
  assert.deepEqual(moved[0].members, ['curve:a', 'curve:b', 'curve:c']);
  assert.equal(groupOf(moved, 'curve:c'), 'g1');
  assert.equal(groupOf(setMembership(moved, 'curve:c', null), 'curve:c'), null);
});

test('renameMember, dropMember and ungroup keep the invariant that a node is in at most one group', () => {
  const { groups } = createGroup([], ['curve:a', 'curve:b'], at);
  const renamed = renameMember(groups, 'curve:a', 'curve:z');
  assert.deepEqual(renamed[0].members, ['curve:z', 'curve:b']);
  assert.deepEqual(dropMember(renamed, 'curve:z')[0].members, ['curve:b']);
  assert.deepEqual(dropMember(dropMember(renamed, 'curve:z'), 'curve:b'), []);
  assert.deepEqual(ungroup(renamed, 'g1'), []);
});

test('pruneGroups drops members that no longer exist and groups left empty', () => {
  const { groups } = createGroup([], ['curve:a', 'curve:ghost'], at);
  const pruned = pruneGroups(groups, [{ id: 'curve:a' }]);
  assert.deepEqual(pruned[0].members, ['curve:a']);
  assert.deepEqual(pruneGroups(groups, []), []);
});

import { projectScope, unprojectConnection } from '../../src/lib/graph/groups.js';
import { configToGraph } from '../../src/lib/graph/model.js';

function zoneFixture() {
  const config = {
    tick_interval_ms: 1000,
    active_profile: 'default',
    profiles: {
      default: {
        sensors: { hot: { type: 'max', inputs: ['hwmon/chipA/temp1'] } },
        curves: {
          cpu: { type: 'point', sensor: 'virtual/hot', points: [[30, 20], [70, 100]] },
          gpu: { type: 'point', sensor: 'nvidia/0/temp', points: [[30, 20], [70, 100]] },
          blend: { type: 'mix', sources: ['cpu', 'gpu'], mode: 'max' },
        },
        assignments: { 'hwmon/chipA/pwm1': 'blend' },
      },
    },
  };
  const inventory = {
    sensors: [
      { id: 'hwmon/chipA/temp1', label: 'CPU', kind: 'temp' },
      { id: 'nvidia/0/temp', label: 'GPU', kind: 'temp' },
    ],
    controls: [{ id: 'hwmon/chipA/pwm1', label: 'CPU fan' }],
  };
  const { nodes, edges } = configToGraph(config, inventory, 'default');
  const groups = [{ id: 'g1', name: 'CPU zone', position: { x: 500, y: 40 }, members: ['virtual:hot', 'curve:cpu'], parent: null }];
  return { nodes, edges, groups };
}

test('root projection replaces members with one group node whose handles are the boundary edges', () => {
  const { nodes, edges, groups } = zoneFixture();
  const view = projectScope(nodes, edges, groups, null);

  assert.ok(!view.nodes.some((node) => node.id === 'virtual:hot' || node.id === 'curve:cpu'));
  const group = view.nodes.find((node) => node.id === 'group:g1');
  assert.equal(group.type, 'group');
  assert.deepEqual(group.position, { x: 500, y: 40 });
  assert.deepEqual(group.data.group.inputs.map((port) => port.handle), ['virtual:hot|in-0']);
  assert.deepEqual(group.data.group.outputs.map((port) => port.handle), ['curve:cpu|out']);
  assert.equal(group.data.group.inputs[0].kind, 'temp');
  assert.equal(group.data.group.outputs[0].kind, 'duty');
  assert.equal(group.data.group.inputs[0].label, 'hot · in-0');

  const inbound = view.edges.find((edge) => edge.target === 'group:g1');
  assert.equal(inbound.source, 'sensor:hwmon/chipA');
  assert.equal(inbound.targetHandle, 'virtual:hot|in-0');
  const outbound = view.edges.find((edge) => edge.source === 'group:g1');
  assert.equal(outbound.target, 'combine:blend');
  assert.equal(outbound.sourceHandle, 'curve:cpu|out');
  assert.ok(!view.edges.some((edge) => edge.source === 'virtual:hot'), 'internal edge must be dropped');
  const flatIds = new Set(edges.map((edge) => edge.id));
  for (const edge of view.edges) assert.ok(flatIds.has(edge.id), `${edge.id} is not a flat edge id`);
});

test('group projection shows members plus one port per outside endpoint, inputs left and outputs right', () => {
  const { nodes, edges, groups } = zoneFixture();
  const view = projectScope(nodes, edges, groups, 'g1');

  assert.deepEqual(view.nodes.filter((node) => node.type !== 'port').map((node) => node.id).sort(), ['curve:cpu', 'virtual:hot']);
  const ports = view.nodes.filter((node) => node.type === 'port');
  assert.deepEqual(ports.map((node) => node.id).sort(), [
    'port:in:sensor:hwmon/chipA:hwmon/chipA/temp1',
    'port:out:combine:blend:in-0',
  ]);
  const input = ports.find((node) => node.data.port.direction === 'in');
  const output = ports.find((node) => node.data.port.direction === 'out');
  const memberXs = view.nodes.filter((node) => node.type !== 'port').map((node) => node.position.x);
  assert.ok(input.position.x < Math.min(...memberXs));
  assert.ok(output.position.x > Math.max(...memberXs));
  assert.equal(input.data.port.label, 'chipA · CPU');
  assert.equal(output.data.port.label, 'blend · in-0');

  const inEdge = view.edges.find((edge) => edge.source === input.id);
  assert.equal(inEdge.sourceHandle, 'port');
  assert.equal(inEdge.target, 'virtual:hot');
  const outEdge = view.edges.find((edge) => edge.target === output.id);
  assert.equal(outEdge.targetHandle, 'port');
  assert.equal(outEdge.source, 'curve:cpu');
  assert.ok(view.edges.some((edge) => edge.source === 'virtual:hot' && edge.target === 'curve:cpu'), 'internal edge stays');
});

test('projectScope with an unknown scope falls back to the root view', () => {
  const { nodes, edges, groups } = zoneFixture();
  assert.deepEqual(projectScope(nodes, edges, groups, 'g9'), projectScope(nodes, edges, groups, null));
});

test('unprojectConnection maps group handles and ports back to flat endpoints and refuses wrong-way ports', () => {
  assert.deepEqual(
    unprojectConnection({ source: 'sensor:nvidia/0', sourceHandle: 'nvidia/0/temp', target: 'group:g1', targetHandle: 'curve:cpu|sensor' }),
    { source: 'sensor:nvidia/0', sourceHandle: 'nvidia/0/temp', target: 'curve:cpu', targetHandle: 'sensor' }
  );
  assert.deepEqual(
    unprojectConnection({ source: 'group:g1', sourceHandle: 'curve:cpu|out', target: 'combine:blend', targetHandle: 'in-1' }),
    { source: 'curve:cpu', sourceHandle: 'out', target: 'combine:blend', targetHandle: 'in-1' }
  );
  assert.deepEqual(
    unprojectConnection({ source: 'port:in:sensor:nvidia/0:nvidia/0/temp', sourceHandle: 'port', target: 'curve:cpu', targetHandle: 'sensor' }),
    { source: 'sensor:nvidia/0', sourceHandle: 'nvidia/0/temp', target: 'curve:cpu', targetHandle: 'sensor' }
  );
  assert.equal(unprojectConnection({ source: 'port:out:combine:blend:in-0', sourceHandle: 'port', target: 'curve:cpu', targetHandle: 'sensor' }), null);
  assert.equal(unprojectConnection({ source: 'curve:cpu', sourceHandle: 'out', target: 'port:in:sensor:nvidia/0:nvidia/0/temp', targetHandle: 'port' }), null);
});
