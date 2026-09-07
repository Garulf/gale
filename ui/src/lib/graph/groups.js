import { shortDevice } from '../dashboard.js';

const GROUP_PREFIX = 'group:';
const PORT_PREFIX = 'port:';
const DECL_PREFIX = 'decl:';

export function groupNodeId(groupId) {
  return `${GROUP_PREFIX}${groupId}`;
}

export function isGroupNodeId(id) {
  return typeof id === 'string' && id.startsWith(GROUP_PREFIX);
}

export function groupIdOf(nodeId) {
  return nodeId.slice(GROUP_PREFIX.length);
}

export function portsNodeId(groupId, direction) {
  return `${PORT_PREFIX}${direction}:${groupId}`;
}

const PORTS_NODE = /^port:(in|out):(.+)$/;

export function isPortsNodeId(id) {
  return typeof id === 'string' && PORTS_NODE.test(id);
}

export function portsNodeDirection(id) {
  const match = PORTS_NODE.exec(id);
  return match ? match[1] : null;
}

export function portsNodeGroupId(id) {
  const match = PORTS_NODE.exec(id);
  return match ? match[2] : null;
}

export function isPortNodeId(id) {
  return isPortsNodeId(id);
}

const DECLARATION_EDGE = /^decl:(in|out):(.+)$/;

export function declarationEdgeId(direction, node, handle) {
  return `${DECL_PREFIX}${direction}:${groupHandleId(node, handle)}`;
}

export function isDeclarationEdgeId(id) {
  return typeof id === 'string' && DECLARATION_EDGE.test(id);
}

export function parseDeclarationEdgeId(id) {
  const match = DECLARATION_EDGE.exec(id);
  if (!match) return null;
  const { nodeId, handle } = parseGroupHandleId(match[2]);
  return { direction: match[1], node: nodeId, handle };
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
  const keep = (port) => !memberIds.includes(port.node);
  return groups.map((group) => ({
    ...group,
    members: group.members.filter((member) => !memberIds.includes(member)),
    inputs: declaredPorts(group, 'in').filter(keep),
    outputs: declaredPorts(group, 'out').filter(keep),
  }));
}

export function createGroup(groups, memberIds, position, name) {
  const members = memberIds.filter((id) => !isGroupNodeId(id) && !isPortsNodeId(id));
  if (members.length === 0) return { groups, id: null };
  const id = nextGroupId(groups);
  const group = {
    id,
    name: name || `Group ${id.slice(1)}`,
    position: { x: position.x, y: position.y },
    members,
    inputs: [],
    outputs: [],
    parent: null,
  };
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
  const rename = (port) => (port.node === oldId ? { ...port, node: newId } : port);
  return groups.map((group) => ({
    ...group,
    members: group.members.map((member) => (member === oldId ? newId : member)),
    inputs: declaredPorts(group, 'in').map(rename),
    outputs: declaredPorts(group, 'out').map(rename),
  }));
}

export function dropMember(groups, nodeId) {
  return setMembership(groups, nodeId, null);
}

export function pruneGroups(groups, nodes) {
  const known = new Set(nodes.map((node) => node.id));
  return withoutEmpty(groups.map((group) => ({ ...group, members: group.members.filter((member) => known.has(member)) })));
}

export const PORT_COLUMN_OFFSET = 316;

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

export function endpointDisplayLabel(nodes, nodeId, handle) {
  return endpointLabel(new Map(nodes.map((node) => [node.id, node])), nodeId, handle);
}

function memberIndex(group) {
  return new Map(group.members.map((member, index) => [member, index]));
}

function nodeTypeOf(nodes, nodeId) {
  const node = nodes.find((candidate) => candidate.id === nodeId);
  if (node) return node.type;
  const kind = nodeId.slice(0, nodeId.indexOf(':'));
  if (kind === 'sensor') return 'deviceSensor';
  if (kind === 'control') return 'deviceControl';
  return kind;
}

export function portKind(nodes, nodeId, handle, direction) {
  const type = nodeTypeOf(nodes, nodeId);
  if (direction === 'in') return type === 'curve' || type === 'virtual' ? 'temp' : 'duty';
  return type === 'virtual' || type === 'deviceSensor' ? 'temp' : 'duty';
}

function declaredPorts(group, direction) {
  const declared = direction === 'in' ? group.inputs : group.outputs;
  return Array.isArray(declared) ? declared : [];
}

function boundaryEdges(edges, members, direction) {
  return edges.filter((edge) =>
    direction === 'in'
      ? members.has(edge.target) && !members.has(edge.source)
      : members.has(edge.source) && !members.has(edge.target)
  );
}

function endpointOf(edge, direction) {
  return direction === 'in'
    ? { node: edge.target, handle: edge.targetHandle }
    : { node: edge.source, handle: edge.sourceHandle };
}

function outsideOf(edge, direction) {
  return direction === 'in'
    ? { node: edge.source, handle: edge.sourceHandle }
    : { node: edge.target, handle: edge.targetHandle };
}

function boundaryEdgeFor(edges, members, direction, node, handle) {
  return (
    boundaryEdges(edges, members, direction).find((edge) => {
      const endpoint = endpointOf(edge, direction);
      return endpoint.node === node && endpoint.handle === handle;
    }) || null
  );
}

function portEntry(byId, nodes, direction, node, handle, name, wired, implied, outside) {
  return {
    node,
    handle,
    name: name || null,
    label: name || endpointLabel(byId, node, handle),
    kind: portKind(nodes, node, handle, direction),
    wired,
    implied,
    outside,
  };
}

function portsFor(group, nodes, edges, direction) {
  const byId = new Map(nodes.map((node) => [node.id, node]));
  const members = new Set(group.members);
  const boundary = boundaryEdges(edges, members, direction);
  const order = memberIndex(group);
  const seen = new Set();
  const result = [];

  for (const port of declaredPorts(group, direction)) {
    if (!members.has(port.node)) continue;
    const key = groupHandleId(port.node, port.handle);
    if (seen.has(key)) continue;
    seen.add(key);
    const edge = boundary.find((candidate) => {
      const endpoint = endpointOf(candidate, direction);
      return endpoint.node === port.node && endpoint.handle === port.handle;
    });
    result.push(
      portEntry(byId, nodes, direction, port.node, port.handle, port.name, Boolean(edge), false, edge ? outsideOf(edge, direction) : null)
    );
  }

  const implied = [];
  for (const edge of boundary) {
    const endpoint = endpointOf(edge, direction);
    const key = groupHandleId(endpoint.node, endpoint.handle);
    if (seen.has(key)) continue;
    seen.add(key);
    implied.push(portEntry(byId, nodes, direction, endpoint.node, endpoint.handle, null, true, true, outsideOf(edge, direction)));
  }
  implied.sort((a, b) => (order.get(a.node) - order.get(b.node)) || a.handle.localeCompare(b.handle));

  return [...result, ...implied];
}

// `outside` is the far endpoint of the first boundary edge for that port, so an output wired to
// several outside targets stays one row and `replace-source` rewrites that first edge.
export function effectivePorts(group, nodes, edges) {
  return {
    inputs: portsFor(group, nodes, edges, 'in'),
    outputs: portsFor(group, nodes, edges, 'out'),
  };
}

function mapGroup(groups, groupId, updater) {
  return groups.map((group) => (group.id === groupId ? updater(group) : group));
}

function listOf(group, direction) {
  return declaredPorts(group, direction);
}

function withList(group, direction, list) {
  return direction === 'in' ? { ...group, inputs: list } : { ...group, outputs: list };
}

function declarePort(groups, groupId, direction, node, handle) {
  return mapGroup(groups, groupId, (group) => {
    const list = listOf(group, direction);
    if (list.some((port) => port.node === node && port.handle === handle)) return group;
    return withList(group, direction, [...list, { node, handle, name: null }]);
  });
}

export function declareInput(groups, groupId, node, handle) {
  return declarePort(groups, groupId, 'in', node, handle);
}

export function declareOutput(groups, groupId, node, handle) {
  return declarePort(groups, groupId, 'out', node, handle);
}

export function undeclarePort(groups, groupId, direction, node, handle) {
  return mapGroup(groups, groupId, (group) =>
    withList(group, direction, listOf(group, direction).filter((port) => !(port.node === node && port.handle === handle)))
  );
}

export function retargetPort(groups, groupId, direction, fromNode, fromHandle, toNode, toHandle) {
  return mapGroup(groups, groupId, (group) => {
    const list = listOf(group, direction);
    if (list.some((port) => port.node === toNode && port.handle === toHandle)) return group;
    return withList(
      group,
      direction,
      list.map((port) =>
        port.node === fromNode && port.handle === fromHandle ? { ...port, node: toNode, handle: toHandle } : port
      )
    );
  });
}

export function retargetInput(groups, groupId, fromNode, fromHandle, toNode, toHandle) {
  return retargetPort(groups, groupId, 'in', fromNode, fromHandle, toNode, toHandle);
}

export function retargetOutput(groups, groupId, fromNode, fromHandle, toNode, toHandle) {
  return retargetPort(groups, groupId, 'out', fromNode, fromHandle, toNode, toHandle);
}

export function dropNodePorts(groups, nodeId) {
  let changed = false;
  const next = groups.map((group) => {
    const inputs = declaredPorts(group, 'in').filter((port) => port.node !== nodeId);
    const outputs = declaredPorts(group, 'out').filter((port) => port.node !== nodeId);
    if (inputs.length === declaredPorts(group, 'in').length && outputs.length === declaredPorts(group, 'out').length) return group;
    changed = true;
    return { ...group, inputs, outputs };
  });
  return changed ? next : groups;
}

export function renamePort(groups, groupId, direction, node, handle, name) {
  const trimmed = typeof name === 'string' ? name.trim() : '';
  return mapGroup(groups, groupId, (group) =>
    withList(
      group,
      direction,
      listOf(group, direction).map((port) =>
        port.node === node && port.handle === handle ? { ...port, name: trimmed || null } : port
      )
    )
  );
}

function rootRow(port) {
  return { handle: groupHandleId(port.node, port.handle), label: port.label, kind: port.kind, wired: port.wired };
}

function projectRoot(nodes, edges, groups) {
  const groupByMember = new Map();
  for (const group of groups) for (const member of group.members) groupByMember.set(member, group);

  const viewEdges = [];
  for (const edge of edges) {
    const sourceGroup = groupByMember.get(edge.source) || null;
    const targetGroup = groupByMember.get(edge.target) || null;
    if (sourceGroup && sourceGroup === targetGroup) continue;
    let projected = edge;
    if (sourceGroup) {
      projected = { ...projected, source: groupNodeId(sourceGroup.id), sourceHandle: groupHandleId(edge.source, edge.sourceHandle) };
    }
    if (targetGroup) {
      projected = { ...projected, target: groupNodeId(targetGroup.id), targetHandle: groupHandleId(edge.target, edge.targetHandle) };
    }
    viewEdges.push(projected);
  }

  const viewNodes = nodes.filter((node) => !groupByMember.has(node.id)).map((node) => ({ ...node }));
  for (const group of groups) {
    const ports = effectivePorts(group, nodes, edges);
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
          inputs: ports.inputs.map(rootRow),
          outputs: ports.outputs.map(rootRow),
        },
      },
    });
  }
  return { nodes: viewNodes, edges: viewEdges };
}

function portsRow(port) {
  return { ...port, handle: groupHandleId(port.node, port.handle) };
}

function portsNode(group, direction, ports, memberNodes) {
  const xs = memberNodes.map((node) => node.position.x);
  const ys = memberNodes.map((node) => node.position.y);
  const offset = direction === 'in' ? -PORT_COLUMN_OFFSET : PORT_COLUMN_OFFSET;
  const anchor = xs.length ? (direction === 'in' ? Math.min(...xs) : Math.max(...xs)) : 0;
  return {
    id: portsNodeId(group.id, direction),
    type: direction === 'in' ? 'groupInputs' : 'groupOutputs',
    position: { x: anchor + offset, y: ys.length ? Math.min(...ys) : 0 },
    hidden: false,
    data: { ports: { direction, groupId: group.id, rows: ports.map(portsRow) } },
  };
}

function declarationEdge(group, direction, port) {
  const handle = groupHandleId(port.node, port.handle);
  const base = {
    id: declarationEdgeId(direction, port.node, port.handle),
    type: 'gale',
    class: `${port.kind} declared`,
    data: { kind: port.kind, declared: true },
  };
  return direction === 'in'
    ? { ...base, source: portsNodeId(group.id, 'in'), sourceHandle: handle, target: port.node, targetHandle: port.handle }
    : { ...base, source: port.node, sourceHandle: port.handle, target: portsNodeId(group.id, 'out'), targetHandle: handle };
}

function projectGroup(nodes, edges, groups, scope) {
  const group = groups.find((candidate) => candidate.id === scope);
  if (!group) return projectRoot(nodes, edges, groups);
  const members = new Set(group.members);
  const ports = effectivePorts(group, nodes, edges);
  const viewEdges = [];

  for (const edge of edges) {
    const sourceIn = members.has(edge.source);
    const targetIn = members.has(edge.target);
    if (!sourceIn && !targetIn) continue;
    if (sourceIn && targetIn) {
      viewEdges.push(edge);
    } else if (targetIn) {
      viewEdges.push({ ...edge, source: portsNodeId(group.id, 'in'), sourceHandle: groupHandleId(edge.target, edge.targetHandle) });
    } else {
      viewEdges.push({ ...edge, target: portsNodeId(group.id, 'out'), targetHandle: groupHandleId(edge.source, edge.sourceHandle) });
    }
  }

  for (const port of ports.inputs) if (!port.wired) viewEdges.push(declarationEdge(group, 'in', port));
  for (const port of ports.outputs) if (!port.wired) viewEdges.push(declarationEdge(group, 'out', port));

  const memberNodes = nodes.filter((node) => members.has(node.id));
  return {
    nodes: [
      ...memberNodes.map((node) => ({ ...node })),
      portsNode(group, 'in', ports.inputs, memberNodes),
      portsNode(group, 'out', ports.outputs, memberNodes),
    ],
    edges: viewEdges,
  };
}

export function projectScope(nodes, edges, groups, scope) {
  return scope ? projectGroup(nodes, edges, groups, scope) : projectRoot(nodes, edges, groups);
}

export const NEW_PORT_HANDLE = 'new';

function unprojectAtRoot(connection) {
  let { source, sourceHandle, target, targetHandle } = connection;
  if (isGroupNodeId(source)) {
    if (typeof sourceHandle !== 'string') return null;
    ({ nodeId: source, handle: sourceHandle } = parseGroupHandleId(sourceHandle));
  }
  if (isGroupNodeId(target)) {
    if (typeof targetHandle !== 'string') return null;
    ({ nodeId: target, handle: targetHandle } = parseGroupHandleId(targetHandle));
  }
  return { kind: 'edge', source, sourceHandle, target, targetHandle };
}

function findPort(ports, node, handle) {
  return ports.find((port) => port.node === node && port.handle === handle) || null;
}

function unprojectInGroup(connection, context) {
  const { source, sourceHandle, target, targetHandle } = connection;
  const sourceIsPorts = isPortsNodeId(source);
  const targetIsPorts = isPortsNodeId(target);
  if (sourceIsPorts && targetIsPorts) return null;
  if (sourceIsPorts && portsNodeDirection(source) !== 'in') return null;
  if (targetIsPorts && portsNodeDirection(target) !== 'out') return null;
  if (typeof sourceHandle !== 'string' || typeof targetHandle !== 'string') return null;

  const groupId = portsNodeGroupId(sourceIsPorts ? source : target);
  const group = (context.groups || []).find((candidate) => candidate.id === groupId);
  if (!group) return null;
  const nodes = context.nodes || [];
  const edges = context.edges || [];
  const members = new Set(group.members);
  const ports = effectivePorts(group, nodes, edges);

  if (sourceIsPorts) {
    if (!members.has(target)) return null;
    if (sourceHandle === NEW_PORT_HANDLE) return { kind: 'declare', direction: 'in', groupId, node: target, handle: targetHandle };
    const { nodeId, handle } = parseGroupHandleId(sourceHandle);
    const port = findPort(ports.inputs, nodeId, handle);
    if (!port) return null;
    if (port.wired) {
      return { kind: 'edge', source: port.outside.node, sourceHandle: port.outside.handle, target, targetHandle };
    }
    return { kind: 'retarget', direction: 'in', groupId, fromNode: nodeId, fromHandle: handle, toNode: target, toHandle: targetHandle };
  }

  if (!members.has(source)) return null;
  if (targetHandle === NEW_PORT_HANDLE) return { kind: 'declare', direction: 'out', groupId, node: source, handle: sourceHandle };
  const { nodeId, handle } = parseGroupHandleId(targetHandle);
  const port = findPort(ports.outputs, nodeId, handle);
  if (!port) return null;
  if (port.wired) {
    const edge = boundaryEdgeFor(edges, members, 'out', nodeId, handle);
    if (!edge) return null;
    return { kind: 'replace-source', edgeId: edge.id, source, sourceHandle };
  }
  return { kind: 'retarget', direction: 'out', groupId, fromNode: nodeId, fromHandle: handle, toNode: source, toHandle: sourceHandle };
}

export function unprojectConnection(connection, context = null) {
  const { source, target } = connection;
  if (isPortsNodeId(source) || isPortsNodeId(target)) {
    return context ? unprojectInGroup(connection, context) : null;
  }
  return unprojectAtRoot(connection);
}
