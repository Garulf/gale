import { shortDevice } from '../dashboard.js';

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

export const PORT_COLUMN_OFFSET = 316;
export const PORT_STACK = 96;

function displayName(node) {
  if (!node) return '?';
  if (node.type === 'deviceSensor') return shortDevice(node.data.deviceSensor.device);
  if (node.type === 'deviceControl') return shortDevice(node.data.deviceControl.device);
  if (node.type === 'virtual') return node.data.virtual.name;
  return node.data[node.type].id;
}

function handleLabel(node, handle) {
  if (!node) return handle;
  const rows = node.type === 'deviceSensor' ? node.data.deviceSensor.rows : node.type === 'deviceControl' ? node.data.deviceControl.rows : null;
  if (!rows) return handle;
  const row = rows.find((candidate) => candidate.handle === handle);
  return row ? row.label : handle;
}

function endpointLabel(byId, nodeId, handle) {
  const node = byId.get(nodeId);
  return `${displayName(node)} · ${handleLabel(node, handle)}`;
}

function memberIndex(group) {
  return new Map(group.members.map((member, index) => [member, index]));
}

function sortedPorts(ports, group) {
  const order = memberIndex(group);
  return [...ports.values()].sort((a, b) => (order.get(a.nodeId) - order.get(b.nodeId)) || a.handle.localeCompare(b.handle));
}

function projectRoot(nodes, edges, groups) {
  const byId = new Map(nodes.map((node) => [node.id, node]));
  const groupByMember = new Map();
  for (const group of groups) for (const member of group.members) groupByMember.set(member, group);
  const ports = new Map(groups.map((group) => [group.id, { inputs: new Map(), outputs: new Map() }]));

  const viewEdges = [];
  for (const edge of edges) {
    const sourceGroup = groupByMember.get(edge.source) || null;
    const targetGroup = groupByMember.get(edge.target) || null;
    if (sourceGroup && sourceGroup === targetGroup) continue;
    let projected = edge;
    if (sourceGroup) {
      const handle = groupHandleId(edge.source, edge.sourceHandle);
      ports.get(sourceGroup.id).outputs.set(handle, { handle, nodeId: edge.source, label: endpointLabel(byId, edge.source, edge.sourceHandle), kind: edge.data.kind });
      projected = { ...projected, source: groupNodeId(sourceGroup.id), sourceHandle: handle };
    }
    if (targetGroup) {
      const handle = groupHandleId(edge.target, edge.targetHandle);
      ports.get(targetGroup.id).inputs.set(handle, { handle, nodeId: edge.target, label: endpointLabel(byId, edge.target, edge.targetHandle), kind: edge.data.kind });
      projected = { ...projected, target: groupNodeId(targetGroup.id), targetHandle: handle };
    }
    viewEdges.push(projected);
  }

  const viewNodes = nodes.filter((node) => !groupByMember.has(node.id));
  for (const group of groups) {
    const own = ports.get(group.id);
    viewNodes.push({
      id: groupNodeId(group.id),
      type: 'group',
      position: { x: group.position.x, y: group.position.y },
      hidden: false,
      data: {
        group: {
          id: group.id,
          name: group.name,
          members: group.members,
          inputs: sortedPorts(own.inputs, group).map(({ handle, label, kind }) => ({ handle, label, kind })),
          outputs: sortedPorts(own.outputs, group).map(({ handle, label, kind }) => ({ handle, label, kind })),
        },
      },
    });
  }
  return { nodes: viewNodes, edges: viewEdges };
}

function placePorts(ports, members) {
  const xs = members.map((node) => node.position.x);
  const ys = members.map((node) => node.position.y);
  const left = (xs.length ? Math.min(...xs) : 0) - PORT_COLUMN_OFFSET;
  const right = (xs.length ? Math.max(...xs) : 0) + PORT_COLUMN_OFFSET;
  const top = ys.length ? Math.min(...ys) : 0;
  let inputs = 0;
  let outputs = 0;
  return ports.map((port) => {
    const isInput = port.data.port.direction === 'in';
    const row = isInput ? inputs++ : outputs++;
    return { ...port, position: { x: isInput ? left : right, y: top + row * PORT_STACK } };
  });
}

function projectGroup(nodes, edges, groups, scope) {
  const group = groups.find((candidate) => candidate.id === scope);
  if (!group) return projectRoot(nodes, edges, groups);
  const byId = new Map(nodes.map((node) => [node.id, node]));
  const members = new Set(group.members);
  const viewEdges = [];
  const ports = new Map();

  const port = (direction, nodeId, handle, kind) => {
    const id = portNodeId(nodeId, handle, direction);
    if (!ports.has(id)) {
      ports.set(id, { id, type: 'port', position: { x: 0, y: 0 }, hidden: false, data: { port: { direction, nodeId, handle, label: endpointLabel(byId, nodeId, handle), kind } } });
    }
    return id;
  };

  for (const edge of edges) {
    const sourceIn = members.has(edge.source);
    const targetIn = members.has(edge.target);
    if (!sourceIn && !targetIn) continue;
    if (sourceIn && targetIn) {
      viewEdges.push(edge);
    } else if (targetIn) {
      viewEdges.push({ ...edge, source: port('in', edge.source, edge.sourceHandle, edge.data.kind), sourceHandle: 'port' });
    } else {
      viewEdges.push({ ...edge, target: port('out', edge.target, edge.targetHandle, edge.data.kind), targetHandle: 'port' });
    }
  }

  const memberNodes = nodes.filter((node) => members.has(node.id));
  return { nodes: [...memberNodes, ...placePorts([...ports.values()], memberNodes)], edges: viewEdges };
}

export function projectScope(nodes, edges, groups, scope) {
  return scope ? projectGroup(nodes, edges, groups, scope) : projectRoot(nodes, edges, groups);
}

export function unprojectConnection(connection) {
  let { source, sourceHandle, target, targetHandle } = connection;
  if (isGroupNodeId(source)) ({ nodeId: source, handle: sourceHandle } = parseGroupHandleId(sourceHandle));
  if (isGroupNodeId(target)) ({ nodeId: target, handle: targetHandle } = parseGroupHandleId(targetHandle));
  if (isPortNodeId(source)) {
    const parsed = parsePortNodeId(source);
    if (parsed.direction !== 'in') return null;
    source = parsed.nodeId;
    sourceHandle = parsed.handle;
  }
  if (isPortNodeId(target)) {
    const parsed = parsePortNodeId(target);
    if (parsed.direction !== 'out') return null;
    target = parsed.nodeId;
    targetHandle = parsed.handle;
  }
  return { source, sourceHandle, target, targetHandle };
}
