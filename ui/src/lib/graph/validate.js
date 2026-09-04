import { nodeKind } from './ids.js';

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

export function isValidConnection(connection, nodes, edges) {
  const { source, target } = connection;
  if (source === target) return false;

  const kind = sourceKind(source);
  const accepted = targetAcceptsKind(target);
  if (accepted !== kind) return false;

  if (wouldCycle(connection, edges)) return false;

  return true;
}
