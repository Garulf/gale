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
