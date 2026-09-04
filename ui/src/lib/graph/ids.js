export function deviceOf(id) {
  return id.split('/').slice(0, -1).join('/');
}

export function sensorNodeId(device) {
  return `sensor:${device}`;
}

export function controlNodeId(device) {
  return `control:${device}`;
}

export function virtualNodeId(name) {
  return `virtual:${name}`;
}

export function curveNodeId(id) {
  return `curve:${id}`;
}

export function combineNodeId(id) {
  return `combine:${id}`;
}

export const COMBINE_TYPES = ['mix', 'sync'];

export function isCombineType(curveType) {
  return COMBINE_TYPES.includes(curveType);
}

export function nodeIdForCurveRef(curveId, curvesById) {
  const curve = curvesById ? curvesById[curveId] : undefined;
  if (!curve) return curveNodeId(curveId);
  return isCombineType(curve.type) ? combineNodeId(curveId) : curveNodeId(curveId);
}

export function nodeKind(nodeId) {
  return nodeId.slice(0, nodeId.indexOf(':'));
}

export function nodeName(nodeId) {
  return nodeId.slice(nodeId.indexOf(':') + 1);
}

export function edgeId(source, sourceHandle, target, targetHandle) {
  return `${source}:${sourceHandle}->${target}:${targetHandle}`;
}

export function virtualInputHandles(count) {
  return count === 1 ? ['in'] : Array.from({ length: count }, (_, i) => `in-${i}`);
}
