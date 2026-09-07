import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  groupNodeId, isGroupNodeId, groupIdOf,
  portsNodeId, isPortsNodeId, portsNodeDirection, portsNodeGroupId,
  declarationEdgeId, isDeclarationEdgeId, parseDeclarationEdgeId,
  groupHandleId, parseGroupHandleId,
  groupOf, nextGroupId, createGroup, ungroup, setMembership, renameMember, dropMember, pruneGroups,
} from '../../src/lib/graph/groups.js';

const at = { x: 10, y: 20 };

test('id helpers round-trip node ids that themselves contain colons and slashes', () => {
  assert.equal(groupNodeId('g3'), 'group:g3');
  assert.ok(isGroupNodeId('group:g3'));
  assert.equal(groupIdOf('group:g3'), 'g3');

  const ports = portsNodeId('g3', 'in');
  assert.equal(ports, 'port:in:g3');
  assert.ok(isPortsNodeId(ports));
  assert.equal(portsNodeDirection(ports), 'in');
  assert.equal(portsNodeGroupId(ports), 'g3');
  assert.equal(portsNodeDirection('curve:cpu'), null);
  assert.ok(!isPortsNodeId('port:sideways:g3'));

  const decl = declarationEdgeId('out', 'combine:radiator_zone', 'out');
  assert.equal(decl, 'decl:out:combine:radiator_zone|out');
  assert.ok(isDeclarationEdgeId(decl));
  assert.ok(!isDeclarationEdgeId('curve:cpu:out->control:x:y'));
  assert.deepEqual(parseDeclarationEdgeId(decl), { direction: 'out', node: 'combine:radiator_zone', handle: 'out' });

  const handle = groupHandleId('combine:radiator_zone', 'out');
  assert.equal(handle, 'combine:radiator_zone|out');
  assert.deepEqual(parseGroupHandleId(handle), { nodeId: 'combine:radiator_zone', handle: 'out' });
});

test('createGroup takes members away from any other group and numbers ids without reuse gaps', () => {
  const first = createGroup([], ['curve:a', 'curve:b'], at);
  assert.equal(first.id, 'g1');
  assert.deepEqual(first.groups, [{ id: 'g1', name: 'Group 1', position: at, members: ['curve:a', 'curve:b'], inputs: [], outputs: [], parent: null }]);

  const second = createGroup(first.groups, ['curve:b', 'curve:c'], at, 'Case');
  assert.equal(second.id, 'g2');
  assert.deepEqual(second.groups.find((g) => g.id === 'g1').members, ['curve:a']);
  assert.deepEqual(second.groups.find((g) => g.id === 'g2'), { id: 'g2', name: 'Case', position: at, members: ['curve:b', 'curve:c'], inputs: [], outputs: [], parent: null });
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

test('group projection emits one Inputs node left and one Outputs node right with the effective rows', () => {
  const { nodes, edges, groups } = zoneFixture();
  const view = projectScope(nodes, edges, groups, 'g1');

  assert.deepEqual(view.nodes.filter((node) => !isPortsNodeId(node.id)).map((node) => node.id).sort(), ['curve:cpu', 'virtual:hot']);
  const input = view.nodes.find((node) => node.id === 'port:in:g1');
  const output = view.nodes.find((node) => node.id === 'port:out:g1');
  assert.equal(input.type, 'groupInputs');
  assert.equal(output.type, 'groupOutputs');
  assert.equal(input.data.ports.direction, 'in');
  assert.equal(input.data.ports.groupId, 'g1');
  assert.deepEqual(input.data.ports.rows.map((row) => row.handle), ['virtual:hot|in-0']);
  assert.deepEqual(output.data.ports.rows.map((row) => row.handle), ['curve:cpu|out']);
  assert.equal(input.data.ports.rows[0].label, 'hot \u00b7 in-0');
  assert.equal(input.data.ports.rows[0].wired, true);

  const memberXs = view.nodes.filter((node) => !isPortsNodeId(node.id)).map((node) => node.position.x);
  assert.ok(input.position.x < Math.min(...memberXs));
  assert.ok(output.position.x > Math.max(...memberXs));

  const inEdge = view.edges.find((edge) => edge.source === 'port:in:g1');
  assert.equal(inEdge.sourceHandle, 'virtual:hot|in-0');
  assert.equal(inEdge.target, 'virtual:hot');
  assert.equal(inEdge.targetHandle, 'in-0');
  const outEdge = view.edges.find((edge) => edge.target === 'port:out:g1');
  assert.equal(outEdge.targetHandle, 'curve:cpu|out');
  assert.equal(outEdge.source, 'curve:cpu');
  const flatIds = new Set(edges.map((edge) => edge.id));
  assert.ok(flatIds.has(inEdge.id) && flatIds.has(outEdge.id), 'wired boundary edges keep their flat ids');
  assert.ok(view.edges.some((edge) => edge.source === 'virtual:hot' && edge.target === 'curve:cpu'), 'internal edge stays');
});

test('group projection draws a declaration edge for every unwired declared port', () => {
  const { nodes, edges, groups } = zoneFixture();
  const declared = groups.map((group) => ({
    ...group,
    inputs: [{ node: 'curve:cpu', handle: 'sensor', name: null }],
    outputs: [{ node: 'virtual:hot', handle: 'out', name: 'heat' }],
  }));
  const view = projectScope(nodes, edges, declared, 'g1');

  const inRow = view.nodes.find((node) => node.id === 'port:in:g1').data.ports.rows;
  assert.deepEqual(inRow.map((row) => row.handle), ['curve:cpu|sensor', 'virtual:hot|in-0']);
  assert.equal(inRow[0].wired, false);
  assert.equal(inRow[0].implied, false);
  assert.equal(inRow[1].implied, true);

  const declIn = view.edges.find((edge) => edge.id === 'decl:in:curve:cpu|sensor');
  assert.deepEqual(declIn, {
    id: 'decl:in:curve:cpu|sensor',
    type: 'gale',
    class: 'temp declared',
    data: { kind: 'temp', declared: true },
    source: 'port:in:g1',
    sourceHandle: 'curve:cpu|sensor',
    target: 'curve:cpu',
    targetHandle: 'sensor',
  });
  const declOut = view.edges.find((edge) => edge.id === 'decl:out:virtual:hot|out');
  assert.equal(declOut.source, 'virtual:hot');
  assert.equal(declOut.sourceHandle, 'out');
  assert.equal(declOut.target, 'port:out:g1');
  assert.equal(declOut.targetHandle, 'virtual:hot|out');
  assert.equal(declOut.data.kind, 'temp');
  assert.ok(!view.edges.some((edge) => edge.id === 'decl:in:virtual:hot|in-0'), 'wired ports get no declaration edge');
});

test('projectScope with an unknown scope falls back to the root view', () => {
  const { nodes, edges, groups } = zoneFixture();
  assert.deepEqual(projectScope(nodes, edges, groups, 'g9'), projectScope(nodes, edges, groups, null));
});

test('unprojectConnection maps group handles back to flat endpoints at the root', () => {
  assert.deepEqual(
    unprojectConnection({ source: 'sensor:nvidia/0', sourceHandle: 'nvidia/0/temp', target: 'group:g1', targetHandle: 'curve:cpu|sensor' }),
    { kind: 'edge', source: 'sensor:nvidia/0', sourceHandle: 'nvidia/0/temp', target: 'curve:cpu', targetHandle: 'sensor' }
  );
  assert.deepEqual(
    unprojectConnection({ source: 'group:g1', sourceHandle: 'curve:cpu|out', target: 'combine:blend', targetHandle: 'in-1' }),
    { kind: 'edge', source: 'curve:cpu', sourceHandle: 'out', target: 'combine:blend', targetHandle: 'in-1' }
  );
});

test('unprojectConnection returns null when a group endpoint has no handle id', () => {
  assert.equal(
    unprojectConnection({ source: 'group:g1', sourceHandle: null, target: 'curve:cpu', targetHandle: 'sensor' }),
    null
  );
  assert.equal(
    unprojectConnection({ source: 'group:g1', target: 'curve:cpu', targetHandle: 'sensor' }),
    null
  );
  assert.equal(
    unprojectConnection({ source: 'sensor:nvidia/0', sourceHandle: 'nvidia/0/temp', target: 'group:g1', targetHandle: null }),
    null
  );
});

function insideContext(extra = {}) {
  const { nodes, edges, groups } = zoneFixture();
  const declared = groups.map((group) => ({ ...group, inputs: extra.inputs || [], outputs: extra.outputs || [] }));
  return { nodes, edges, groups: declared, scope: 'g1' };
}

test('unprojectConnection turns a new-row connection into a declaration', () => {
  const context = insideContext();
  assert.deepEqual(
    unprojectConnection({ source: 'port:in:g1', sourceHandle: 'new', target: 'curve:cpu', targetHandle: 'sensor' }, context),
    { kind: 'declare', direction: 'in', groupId: 'g1', node: 'curve:cpu', handle: 'sensor' }
  );
  assert.deepEqual(
    unprojectConnection({ source: 'curve:cpu', sourceHandle: 'out', target: 'port:out:g1', targetHandle: 'new' }, context),
    { kind: 'declare', direction: 'out', groupId: 'g1', node: 'curve:cpu', handle: 'out' }
  );
});

test('unprojectConnection fans a wired Inputs row out to another member handle', () => {
  const context = insideContext();
  assert.deepEqual(
    unprojectConnection({ source: 'port:in:g1', sourceHandle: 'virtual:hot|in-0', target: 'curve:cpu', targetHandle: 'sensor' }, context),
    { kind: 'edge', source: 'sensor:hwmon/chipA', sourceHandle: 'hwmon/chipA/temp1', target: 'curve:cpu', targetHandle: 'sensor' }
  );
});

test('unprojectConnection retargets an unwired declared port instead of wiring it', () => {
  const context = insideContext({ inputs: [{ node: 'curve:cpu', handle: 'sensor', name: null }] });
  assert.deepEqual(
    unprojectConnection({ source: 'port:in:g1', sourceHandle: 'curve:cpu|sensor', target: 'virtual:hot', targetHandle: 'in-1' }, context),
    { kind: 'retarget', direction: 'in', groupId: 'g1', fromNode: 'curve:cpu', fromHandle: 'sensor', toNode: 'virtual:hot', toHandle: 'in-1' }
  );
  const outContext = insideContext({ outputs: [{ node: 'virtual:hot', handle: 'out', name: null }] });
  assert.deepEqual(
    unprojectConnection({ source: 'curve:cpu', sourceHandle: 'out', target: 'port:out:g1', targetHandle: 'virtual:hot|out' }, outContext),
    { kind: 'retarget', direction: 'out', groupId: 'g1', fromNode: 'virtual:hot', fromHandle: 'out', toNode: 'curve:cpu', toHandle: 'out' }
  );
});

test('unprojectConnection replaces the source of the boundary edge behind a wired Outputs row', () => {
  const context = insideContext();
  const boundary = context.edges.find((edge) => edge.source === 'curve:cpu' && edge.target === 'combine:blend');
  assert.deepEqual(
    unprojectConnection({ source: 'virtual:hot', sourceHandle: 'out', target: 'port:out:g1', targetHandle: 'curve:cpu|out' }, context),
    { kind: 'replace-source', edgeId: boundary.id, source: 'virtual:hot', sourceHandle: 'out' }
  );
});

test('unprojectConnection refuses wrong-way ports rows, unknown rows and missing context', () => {
  const context = insideContext();
  assert.equal(unprojectConnection({ source: 'port:out:g1', sourceHandle: 'curve:cpu|out', target: 'curve:cpu', targetHandle: 'sensor' }, context), null);
  assert.equal(unprojectConnection({ source: 'curve:cpu', sourceHandle: 'out', target: 'port:in:g1', targetHandle: 'new' }, context), null);
  assert.equal(unprojectConnection({ source: 'port:in:g1', sourceHandle: 'new', target: 'port:out:g1', targetHandle: 'new' }, context), null);
  assert.equal(unprojectConnection({ source: 'port:in:g1', sourceHandle: null, target: 'curve:cpu', targetHandle: 'sensor' }, context), null);
  assert.equal(unprojectConnection({ source: 'port:in:g1', sourceHandle: 'curve:gpu|sensor', target: 'curve:cpu', targetHandle: 'sensor' }, context), null);
  assert.equal(unprojectConnection({ source: 'port:in:g1', sourceHandle: 'new', target: 'combine:blend', targetHandle: 'in-0' }, context), null);
  assert.equal(unprojectConnection({ source: 'port:in:g1', sourceHandle: 'new', target: 'curve:cpu', targetHandle: 'sensor' }), null);
});

test('projectScope never hands a member node object back to the caller', () => {
  const { nodes, edges, groups } = zoneFixture();
  const root = projectScope(nodes, edges, groups, null);
  for (const node of root.nodes) assert.ok(!nodes.includes(node), `${node.id} aliases the flat node`);
  const inside = projectScope(nodes, edges, groups, 'g1');
  for (const node of inside.nodes) assert.ok(!nodes.includes(node), `${node.id} aliases the flat node`);
});

import {
  effectivePorts, portKind, endpointDisplayLabel,
  declareInput, declareOutput, undeclarePort, retargetInput, renamePort,
} from '../../src/lib/graph/groups.js';

test('portKind reads the accepted kind of a member handle in each direction', () => {
  const { nodes } = zoneFixture();
  assert.equal(portKind(nodes, 'curve:cpu', 'sensor', 'in'), 'temp');
  assert.equal(portKind(nodes, 'virtual:hot', 'in-0', 'in'), 'temp');
  assert.equal(portKind(nodes, 'combine:blend', 'in-0', 'in'), 'duty');
  assert.equal(portKind(nodes, 'curve:cpu', 'out', 'out'), 'duty');
  assert.equal(portKind(nodes, 'combine:blend', 'out', 'out'), 'duty');
  assert.equal(portKind(nodes, 'virtual:hot', 'out', 'out'), 'temp');
  assert.equal(portKind(nodes, 'sensor:hwmon/chipA', 'hwmon/chipA/temp1', 'out'), 'temp');
});

test('effectivePorts lists declared ports first, then implied ones, without duplicates', () => {
  const { nodes, edges, groups } = zoneFixture();
  const group = {
    ...groups[0],
    inputs: [
      { node: 'curve:cpu', handle: 'sensor', name: 'coolant' },
      { node: 'virtual:hot', handle: 'in-0', name: null },
    ],
    outputs: [],
  };
  const { inputs, outputs } = effectivePorts(group, nodes, edges);

  assert.deepEqual(inputs.map((port) => `${port.node}|${port.handle}`), ['curve:cpu|sensor', 'virtual:hot|in-0']);
  assert.equal(inputs[0].wired, false);
  assert.equal(inputs[0].implied, false);
  assert.equal(inputs[0].label, 'coolant');
  assert.equal(inputs[0].name, 'coolant');
  assert.equal(inputs[0].kind, 'temp');
  assert.equal(inputs[0].outside, null);

  assert.equal(inputs[1].wired, true);
  assert.equal(inputs[1].implied, false, 'a declared port that is also wired stays a single declared entry');
  assert.deepEqual(inputs[1].outside, { node: 'sensor:hwmon/chipA', handle: 'hwmon/chipA/temp1' });
  assert.equal(inputs[1].label, 'hot \u00b7 in-0');

  assert.deepEqual(outputs.map((port) => `${port.node}|${port.handle}`), ['curve:cpu|out']);
  assert.equal(outputs[0].implied, true);
  assert.equal(outputs[0].wired, true);
  assert.deepEqual(outputs[0].outside, { node: 'combine:blend', handle: 'in-0' });
});

test('effectivePorts on a group with no declarations is exactly the boundary edges', () => {
  const { nodes, edges, groups } = zoneFixture();
  const { inputs, outputs } = effectivePorts(groups[0], nodes, edges);
  assert.deepEqual(inputs.map((port) => port.node), ['virtual:hot']);
  assert.deepEqual(outputs.map((port) => port.node), ['curve:cpu']);
  assert.ok(inputs.every((port) => port.implied && port.wired));
});

test('declare, undeclare, retarget and rename are pure and idempotent where they should be', () => {
  const groups = [{ id: 'g1', name: 'Zone', position: { x: 0, y: 0 }, members: ['curve:cpu'], inputs: [], outputs: [], parent: null }];
  const declared = declareInput(groups, 'g1', 'curve:cpu', 'sensor');
  assert.deepEqual(groups[0].inputs, [], 'input groups are not mutated');
  assert.deepEqual(declared[0].inputs, [{ node: 'curve:cpu', handle: 'sensor', name: null }]);
  assert.deepEqual(declareInput(declared, 'g1', 'curve:cpu', 'sensor')[0].inputs, declared[0].inputs);

  const withOutput = declareOutput(declared, 'g1', 'curve:cpu', 'out');
  assert.deepEqual(withOutput[0].outputs, [{ node: 'curve:cpu', handle: 'out', name: null }]);

  const named = renamePort(withOutput, 'g1', 'in', 'curve:cpu', 'sensor', '  coolant  ');
  assert.equal(named[0].inputs[0].name, 'coolant');
  assert.equal(renamePort(named, 'g1', 'in', 'curve:cpu', 'sensor', '   ')[0].inputs[0].name, null);

  const moved = retargetInput(named, 'g1', 'curve:cpu', 'sensor', 'virtual:hot', 'in-0');
  assert.deepEqual(moved[0].inputs, [{ node: 'virtual:hot', handle: 'in-0', name: 'coolant' }]);
  assert.deepEqual(
    retargetInput(moved, 'g1', 'virtual:hot', 'in-0', 'virtual:hot', 'in-0')[0].inputs,
    moved[0].inputs,
    'retargeting onto an existing port is a no-op'
  );

  assert.deepEqual(undeclarePort(moved, 'g1', 'in', 'virtual:hot', 'in-0')[0].inputs, []);
  assert.deepEqual(undeclarePort(moved, 'g1', 'in', 'curve:ghost', 'sensor')[0].inputs, moved[0].inputs);
  assert.deepEqual(declareInput(groups, 'g2', 'curve:cpu', 'sensor'), groups, 'an unknown group id changes nothing');
});

test('membership changes carry the declared ports with them', () => {
  const declared = [
    {
      id: 'g1',
      name: 'Zone',
      position: { x: 0, y: 0 },
      members: ['curve:cpu', 'virtual:hot'],
      inputs: [{ node: 'curve:cpu', handle: 'sensor', name: 'coolant' }, { node: 'virtual:hot', handle: 'in-0', name: null }],
      outputs: [{ node: 'curve:cpu', handle: 'out', name: null }],
      parent: null,
    },
  ];

  const moved = setMembership(declared, 'curve:cpu', null);
  assert.deepEqual(moved[0].members, ['virtual:hot']);
  assert.deepEqual(moved[0].inputs, [{ node: 'virtual:hot', handle: 'in-0', name: null }]);
  assert.deepEqual(moved[0].outputs, []);
  assert.deepEqual(dropMember(declared, 'curve:cpu')[0].inputs, moved[0].inputs);
  assert.equal(declared[0].inputs.length, 2, 'the input groups are not mutated');

  const renamed = renameMember(declared, 'curve:cpu', 'curve:rad');
  assert.deepEqual(renamed[0].members, ['curve:rad', 'virtual:hot']);
  assert.deepEqual(renamed[0].inputs[0], { node: 'curve:rad', handle: 'sensor', name: 'coolant' });
  assert.deepEqual(renamed[0].outputs, [{ node: 'curve:rad', handle: 'out', name: null }]);
});

test('a declared port whose node is no longer a member is never projected', () => {
  const { nodes, edges, groups } = zoneFixture();
  const stale = groups.map((group) => ({
    ...group,
    inputs: [{ node: 'combine:blend', handle: 'in-0', name: 'stale' }],
    outputs: [],
  }));
  const { inputs } = effectivePorts(stale[0], nodes, edges);
  assert.ok(!inputs.some((port) => port.node === 'combine:blend'));

  const view = projectScope(nodes, edges, stale, 'g1');
  const rows = view.nodes.find((node) => node.id === 'port:in:g1').data.ports.rows;
  assert.ok(!rows.some((row) => row.handle === 'combine:blend|in-0'));
  assert.ok(!view.edges.some((edge) => edge.id === 'decl:in:combine:blend|in-0'));
});

test('an output wired to several outside targets stays one row bound to the first edge', () => {
  const { nodes, edges, groups } = zoneFixture();
  const second = {
    id: 'curve:cpu:out->combine:blend:in-1',
    source: 'curve:cpu',
    sourceHandle: 'out',
    target: 'combine:blend',
    targetHandle: 'in-1',
    data: { kind: 'duty' },
  };
  const fanned = [...edges, second];
  const { outputs } = effectivePorts(groups[0], nodes, fanned);
  assert.equal(outputs.length, 1);
  const first = edges.find((edge) => edge.source === 'curve:cpu' && edge.target === 'combine:blend');
  assert.deepEqual(outputs[0].outside, { node: 'combine:blend', handle: first.targetHandle });

  const context = { nodes, edges: fanned, groups, scope: 'g1' };
  assert.deepEqual(
    unprojectConnection({ source: 'virtual:hot', sourceHandle: 'out', target: 'port:out:g1', targetHandle: 'curve:cpu|out' }, context),
    { kind: 'replace-source', edgeId: first.id, source: 'virtual:hot', sourceHandle: 'out' }
  );
});

test('endpointDisplayLabel names an endpoint the way port rows show it', () => {
  const { nodes } = zoneFixture();
  assert.equal(endpointDisplayLabel(nodes, 'sensor:hwmon/chipA', 'hwmon/chipA/temp1'), 'chipA \u00b7 CPU');
  assert.equal(endpointDisplayLabel(nodes, 'curve:cpu', 'sensor'), 'cpu \u00b7 sensor');
  assert.equal(endpointDisplayLabel(nodes, 'curve:gone', 'out'), '? \u00b7 out');
});
