import {
  virtualNodeId,
  curveNodeId,
  combineNodeId,
  isCombineType,
  isSingleInputVirtualType,
  nodeKind,
  edgeId,
} from './ids.js';
import { defaultVirtualSensor } from '../sensors.js';
import { defaultCurve } from './defaults.js';

export const CURVE_TYPES = ['point', 'flat', 'mix', 'sync', 'trigger', 'target'];

const SENSOR_INPUT_CURVE_TYPES = ['point', 'trigger', 'target'];

function nodeIdFor(kind, name) {
  if (kind === 'virtual') return virtualNodeId(name);
  if (kind === 'combine') return combineNodeId(name);
  return curveNodeId(name);
}

export function nameInUse(nodes, kind, name, exceptId) {
  const candidates = kind === 'virtual' ? [virtualNodeId(name)] : [curveNodeId(name), combineNodeId(name)];
  return nodes.some((node) => node.id !== exceptId && candidates.includes(node.id));
}

function dataFor(nodeType, name, config) {
  if (nodeType === 'virtual') return { virtual: { name, config } };
  return { [nodeType]: { id: name, config } };
}

function repointEdges(edges, oldId, newId, remapIncomingHandle) {
  const result = [];
  for (const edge of edges) {
    let { source, sourceHandle, target, targetHandle } = edge;
    if (source === oldId) source = newId;
    if (target === oldId) {
      target = newId;
      targetHandle = remapIncomingHandle(targetHandle);
      if (targetHandle === null) continue;
    }
    result.push({ ...edge, id: edgeId(source, sourceHandle, target, targetHandle), source, sourceHandle, target, targetHandle });
  }
  return result;
}

function keepHandle(handle) {
  return handle;
}

export function renameNode(nodes, edges, id, name) {
  const kind = nodeKind(id);
  const newId = nodeIdFor(kind, name);
  const nodeType = kind === 'virtual' ? 'virtual' : kind;
  return {
    id: newId,
    nodes: nodes.map((node) => {
      if (node.id !== id) return node;
      return { ...node, id: newId, data: dataFor(nodeType, name, node.data[nodeType].config) };
    }),
    edges: repointEdges(edges, id, newId, keepHandle),
  };
}

function virtualHandleRemap(oldType, newType) {
  const wasSingle = isSingleInputVirtualType(oldType);
  const isSingle = isSingleInputVirtualType(newType);
  if (wasSingle === isSingle) return keepHandle;
  if (isSingle) return (handle) => (handle === 'in-0' ? 'in' : null);
  return (handle) => (handle === 'in' ? 'in-0' : null);
}

export function changeVirtualType(nodes, edges, id, type) {
  const node = nodes.find((candidate) => candidate.id === id);
  const oldType = node.data.virtual.config.type;
  if (oldType === type) return { id, nodes, edges };
  return {
    id,
    nodes: nodes.map((candidate) => {
      if (candidate.id !== id) return candidate;
      return { ...candidate, data: dataFor('virtual', candidate.data.virtual.name, defaultVirtualSensor(type)) };
    }),
    edges: repointEdges(edges, id, id, virtualHandleRemap(oldType, type)),
  };
}

function curveHandleRemap(oldType, newType) {
  if (SENSOR_INPUT_CURVE_TYPES.includes(oldType) && SENSOR_INPUT_CURVE_TYPES.includes(newType)) {
    return keepHandle;
  }
  if (oldType === 'mix' && newType === 'sync') return (handle) => (handle === 'in-0' ? 'in' : null);
  if (oldType === 'sync' && newType === 'mix') return (handle) => (handle === 'in' ? 'in-0' : null);
  if (oldType === newType) return keepHandle;
  return () => null;
}

export function changeCurveType(nodes, edges, id, type) {
  const node = nodes.find((candidate) => candidate.id === id);
  const oldType = node.data[node.type].config.type;
  if (oldType === type) return { id, nodes, edges };
  const name = node.data[node.type].id;
  const nodeType = isCombineType(type) ? 'combine' : 'curve';
  const newId = nodeIdFor(nodeType, name);
  return {
    id: newId,
    nodes: nodes.map((candidate) => {
      if (candidate.id !== id) return candidate;
      return { ...candidate, id: newId, type: nodeType, data: dataFor(nodeType, name, defaultCurve(type)) };
    }),
    edges: repointEdges(edges, id, newId, curveHandleRemap(oldType, type)),
  };
}
