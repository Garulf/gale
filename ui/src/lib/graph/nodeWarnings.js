import { nodeKind, deviceOf, isZeroInputVirtualType, isSingleInputCombineType } from './ids.js';
import { virtualId } from '../sensors.js';
import { isCurveInputKind } from './liveValues.js';

const ID_CHAR = /[A-Za-z0-9_./-]/;
const TEMP_CONSUMERS = new Set(['curve', 'virtual']);
const SENSOR_CURVE_TYPES = new Set(['point', 'linear', 'trigger', 'target']);

function containsId(message, id) {
  let from = 0;
  while (from <= message.length - id.length) {
    const at = message.indexOf(id, from);
    if (at === -1) return false;
    const before = message[at - 1];
    const after = message[at + id.length];
    if (!(before && ID_CHAR.test(before)) && !(after && ID_CHAR.test(after))) return true;
    from = at + 1;
  }
  return false;
}

function pathTokens(message) {
  return (message.match(/[A-Za-z0-9_][A-Za-z0-9_./-]*/g) || []).filter((token) => token.includes('/'));
}

function nodesByType(nodes, type) {
  return nodes.filter((node) => node.type === type);
}

export function attributeWarning(message, nodes, edges) {
  const attributed = new Set();
  const devices = nodesByType(nodes, 'deviceSensor');
  const referencedDevices = new Set(pathTokens(message).map(deviceOf));

  for (const node of devices) {
    const { device, rows } = node.data.deviceSensor;
    if (referencedDevices.has(device) || rows.some((row) => containsId(message, row.handle))) {
      attributed.add(node.id);
    }
  }

  for (const node of nodesByType(nodes, 'virtual')) {
    const name = node.data.virtual.name;
    if (containsId(message, virtualId(name)) || message.includes(`'${name}'`)) attributed.add(node.id);
  }

  for (const edge of edges) {
    if (nodeKind(edge.source) !== 'sensor') continue;
    if (containsId(message, edge.sourceHandle)) attributed.add(edge.target);
  }

  return [...attributed];
}

function hasEdgeInto(edges, target, targetHandle) {
  return edges.some(
    (edge) => edge.target === target && (targetHandle === undefined || edge.targetHandle === targetHandle)
  );
}

export function unwiredRequiredInputs(nodes, edges) {
  const flagged = [];
  for (const node of nodes) {
    if (node.type === 'curve') {
      const type = node.data.curve.config.type;
      if (SENSOR_CURVE_TYPES.has(type) && !hasEdgeInto(edges, node.id, 'sensor')) {
        flagged.push({ nodeId: node.id, message: 'sensor input is not wired' });
      }
    } else if (node.type === 'combine') {
      const type = node.data.combine.config.type;
      if (isSingleInputCombineType(type) && !hasEdgeInto(edges, node.id, 'in')) {
        flagged.push({ nodeId: node.id, message: 'input is not wired' });
      } else if (type === 'mix' && !hasEdgeInto(edges, node.id)) {
        flagged.push({ nodeId: node.id, message: 'no inputs are wired' });
      }
    } else if (
      node.type === 'virtual' &&
      !isZeroInputVirtualType(node.data.virtual.config.type) &&
      !hasEdgeInto(edges, node.id)
    ) {
      flagged.push({ nodeId: node.id, message: 'no inputs are wired' });
    }
  }
  return flagged;
}

function sensorRow(nodes, nodeId, handle) {
  const node = nodes.find((candidate) => candidate.id === nodeId && candidate.type === 'deviceSensor');
  if (!node) return undefined;
  return node.data.deviceSensor.rows.find((row) => row.handle === handle);
}

export function kindMismatchedEdges(nodes, edges) {
  const flagged = [];
  for (const edge of edges) {
    if (nodeKind(edge.source) !== 'sensor' || !TEMP_CONSUMERS.has(nodeKind(edge.target))) continue;
    const row = sensorRow(nodes, edge.source, edge.sourceHandle);
    if (!row || isCurveInputKind(row.kind)) continue;
    flagged.push({
      nodeId: edge.target,
      message: `"${row.label}" is a ${row.kind} reading and cannot drive a curve; rewire "${edge.targetHandle}" from a temperature or metric sensor`,
    });
  }
  return flagged;
}

function addWarning(collected, nodeId, message) {
  if (!collected[nodeId]) collected[nodeId] = [];
  if (!collected[nodeId].includes(message)) collected[nodeId].push(message);
}

export function collectNodeWarnings(nodes, edges, daemonWarnings) {
  const collected = {};
  for (const message of daemonWarnings || []) {
    for (const nodeId of attributeWarning(message, nodes, edges)) addWarning(collected, nodeId, message);
  }
  for (const entry of unwiredRequiredInputs(nodes, edges)) addWarning(collected, entry.nodeId, entry.message);
  for (const entry of kindMismatchedEdges(nodes, edges)) addWarning(collected, entry.nodeId, entry.message);
  return collected;
}
