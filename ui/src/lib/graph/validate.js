import { nodeKind } from './ids.js';
import { isCurveInputKind } from './liveValues.js';

function sourceKind(nodeId) {
  const kind = nodeKind(nodeId);
  return kind === 'sensor' || kind === 'virtual' ? 'temp' : 'duty';
}

function targetAcceptsKind(nodeId) {
  const kind = nodeKind(nodeId);
  if (kind === 'control' || kind === 'combine') return 'duty';
  if (kind === 'virtual' || kind === 'curve') return 'temp';
  return null;
}

export function wouldCycle(candidateEdge, edges) {
  if (nodeKind(candidateEdge.source) !== 'virtual' || nodeKind(candidateEdge.target) !== 'virtual') {
    return false;
  }

  const adjacency = new Map();
  for (const edge of edges) {
    if (
      edge.data &&
      edge.data.kind === 'temp' &&
      nodeKind(edge.source) === 'virtual' &&
      nodeKind(edge.target) === 'virtual'
    ) {
      if (!adjacency.has(edge.source)) adjacency.set(edge.source, []);
      adjacency.get(edge.source).push(edge.target);
    }
  }

  const goal = candidateEdge.source;
  const stack = [candidateEdge.target];
  const visited = new Set();
  while (stack.length > 0) {
    const current = stack.pop();
    if (current === goal) return true;
    if (visited.has(current)) continue;
    visited.add(current);
    for (const next of adjacency.get(current) || []) {
      stack.push(next);
    }
  }
  return false;
}

function sensorRowKind(nodes, nodeId, handle) {
  const node = (nodes || []).find((candidate) => candidate.id === nodeId);
  const rows = node && node.data && node.data.deviceSensor ? node.data.deviceSensor.rows : [];
  const row = rows.find((candidate) => candidate.handle === handle);
  return row ? row.kind : undefined;
}

function sourceIsCurveInput(connection, nodes) {
  if (nodeKind(connection.source) !== 'sensor') return true;
  const kind = sensorRowKind(nodes, connection.source, connection.sourceHandle);
  return isCurveInputKind(kind);
}

export function isValidConnection(connection, nodes, edges) {
  const { source, target } = connection;
  if (source === target) return false;

  const kind = sourceKind(source);
  const accepted = targetAcceptsKind(target);
  if (accepted !== kind) return false;
  if (!sourceIsCurveInput(connection, nodes)) return false;

  if (wouldCycle(connection, edges)) return false;

  return true;
}
