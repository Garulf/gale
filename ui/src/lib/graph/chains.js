import { nodeKind } from './ids.js';

function edgesInto(edges, nodeId) {
  return edges.filter((edge) => edge.target === nodeId);
}

function edgesOutOf(edges, nodeId) {
  return edges.filter((edge) => edge.source === nodeId);
}

function upstreamSteps(nodeId, nodes, edges, seen) {
  if (seen.has(nodeId)) return [];
  seen.add(nodeId);
  const node = nodes.find((candidate) => candidate.id === nodeId);
  if (!node) return [];
  const steps = [];
  for (const edge of edgesInto(edges, nodeId)) {
    const sourceKind = nodeKind(edge.source);
    if (sourceKind === 'sensor' || sourceKind === 'control') {
      steps.push({ nodeId: edge.source, handle: edge.sourceHandle, kind: 'sensor' });
    } else {
      steps.push(...upstreamSteps(edge.source, nodes, edges, seen));
      steps.push({ nodeId: edge.source, handle: 'out', kind: sourceKind });
    }
  }
  return steps;
}

export function stepKey(step) {
  return `${step.nodeId}|${step.handle}`;
}

function dedupe(steps) {
  const seen = new Set();
  return steps.filter((step) => {
    const key = stepKey(step);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

export function buildChains(nodes, edges) {
  const chains = [];
  const covered = new Set();
  const heads = nodes.filter((node) => node.type === 'curve' || node.type === 'combine');
  for (const head of heads) {
    const targets = edgesOutOf(edges, head.id).filter((edge) => nodeKind(edge.target) === 'control');
    if (targets.length === 0) continue;
    const seen = new Set();
    const steps = dedupe([
      ...upstreamSteps(head.id, nodes, edges, seen),
      { nodeId: head.id, handle: 'out', kind: head.type },
      ...targets.map((edge) => ({ nodeId: edge.target, handle: edge.targetHandle, kind: 'control' })),
    ]);
    for (const step of steps) covered.add(step.nodeId);
    chains.push({ id: head.id, steps });
  }
  for (const node of nodes) {
    if (covered.has(node.id) || node.hidden) continue;
    if (node.type === 'deviceSensor' || node.type === 'deviceControl') continue;
    const seen = new Set();
    const steps = dedupe([...upstreamSteps(node.id, nodes, edges, seen), { nodeId: node.id, handle: 'out', kind: node.type }]);
    chains.push({ id: node.id, steps, unassigned: true });
  }
  return chains;
}

export function chainDevices(chains, nodes) {
  const devices = new Set();
  for (const chain of chains) {
    for (const step of chain.steps) {
      const node = nodes.find((candidate) => candidate.id === step.nodeId);
      if (!node) continue;
      if (node.type === 'deviceSensor') devices.add(node.data.deviceSensor.device.split('/')[0]);
      if (node.type === 'deviceControl') devices.add(node.data.deviceControl.device.split('/')[0]);
    }
  }
  return [...devices].sort();
}

export function chainMatchesFilter(chain, nodes, filter, overrides) {
  if (filter === 'all') return true;
  return chain.steps.some((step) => {
    const node = nodes.find((candidate) => candidate.id === step.nodeId);
    if (!node) return false;
    if (filter === 'manual') return node.type === 'deviceControl' && (overrides || []).includes(step.handle);
    if (node.type === 'deviceSensor') return node.data.deviceSensor.device.startsWith(`${filter}/`);
    if (node.type === 'deviceControl') return node.data.deviceControl.device.startsWith(`${filter}/`);
    return false;
  });
}
