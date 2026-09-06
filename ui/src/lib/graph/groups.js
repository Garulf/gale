const GROUP_PREFIX = 'group:';
const PORT_PREFIX = 'port:';

export function groupNodeId(groupId) {
  return `${GROUP_PREFIX}${groupId}`;
}

export function isGroupNodeId(id) {
  return typeof id === 'string' && id.startsWith(GROUP_PREFIX);
}

export function groupIdOf(nodeId) {
  return nodeId.slice(GROUP_PREFIX.length);
}

export function portNodeId(nodeId, handle, direction) {
  return `${PORT_PREFIX}${direction}:${nodeId}:${handle}`;
}

export function isPortNodeId(id) {
  return typeof id === 'string' && id.startsWith(PORT_PREFIX);
}

export function parsePortNodeId(id) {
  const rest = id.slice(PORT_PREFIX.length);
  const direction = rest.slice(0, rest.indexOf(':'));
  const body = rest.slice(direction.length + 1);
  const split = body.lastIndexOf(':');
  return { direction, nodeId: body.slice(0, split), handle: body.slice(split + 1) };
}

export function groupHandleId(nodeId, handle) {
  return `${nodeId}|${handle}`;
}

export function parseGroupHandleId(handleId) {
  const split = handleId.lastIndexOf('|');
  return { nodeId: handleId.slice(0, split), handle: handleId.slice(split + 1) };
}

export function groupOf(groups, nodeId) {
  const group = groups.find((candidate) => candidate.members.includes(nodeId));
  return group ? group.id : null;
}

export function nextGroupId(groups) {
  let n = 1;
  while (groups.some((group) => group.id === `g${n}`)) n += 1;
  return `g${n}`;
}

function withoutEmpty(groups) {
  return groups.filter((group) => group.members.length > 0);
}

function stripMembers(groups, memberIds) {
  return groups.map((group) => ({ ...group, members: group.members.filter((member) => !memberIds.includes(member)) }));
}

export function createGroup(groups, memberIds, position, name) {
  const members = memberIds.filter((id) => !isGroupNodeId(id) && !isPortNodeId(id));
  if (members.length === 0) return { groups, id: null };
  const id = nextGroupId(groups);
  const group = { id, name: name || `Group ${id.slice(1)}`, position: { x: position.x, y: position.y }, members, parent: null };
  return { groups: [...withoutEmpty(stripMembers(groups, members)), group], id };
}

export function ungroup(groups, id) {
  return groups.filter((group) => group.id !== id);
}

export function setMembership(groups, nodeId, groupId) {
  const stripped = stripMembers(groups, [nodeId]);
  const joined = groupId
    ? stripped.map((group) => (group.id === groupId ? { ...group, members: [...group.members, nodeId] } : group))
    : stripped;
  return withoutEmpty(joined);
}

export function renameMember(groups, oldId, newId) {
  return groups.map((group) => ({ ...group, members: group.members.map((member) => (member === oldId ? newId : member)) }));
}

export function dropMember(groups, nodeId) {
  return setMembership(groups, nodeId, null);
}

export function pruneGroups(groups, nodes) {
  const known = new Set(nodes.map((node) => node.id));
  return withoutEmpty(groups.map((group) => ({ ...group, members: group.members.filter((member) => known.has(member)) })));
}
