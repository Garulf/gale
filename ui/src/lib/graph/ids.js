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

export const COMBINE_TYPES = ['mix', 'sync', 'offset'];

export function isCombineType(curveType) {
  return COMBINE_TYPES.includes(curveType);
}

export const SINGLE_INPUT_COMBINE_TYPES = ['sync', 'offset'];

export function isSingleInputCombineType(curveType) {
  return SINGLE_INPUT_COMBINE_TYPES.includes(curveType);
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

export const SINGLE_INPUT_VIRTUAL_TYPES = ['offset', 'delta'];

export function isSingleInputVirtualType(type) {
  return SINGLE_INPUT_VIRTUAL_TYPES.includes(type);
}

export const ZERO_INPUT_VIRTUAL_TYPES = ['webhook'];

export function isZeroInputVirtualType(type) {
  return ZERO_INPUT_VIRTUAL_TYPES.includes(type);
}

export function virtualInputHandles(type, count) {
  if (isZeroInputVirtualType(type)) return [];
  if (isSingleInputVirtualType(type)) return ['in'];
  return Array.from({ length: count }, (_, i) => `in-${i}`);
}

const NUMBERED_INPUT = /^in-(\d+)$/;

export function numberedInputCount(connectedHandles) {
  let connected = 0;
  let highest = -1;
  for (const handle of connectedHandles) {
    const match = NUMBERED_INPUT.exec(handle);
    if (!match) continue;
    connected += 1;
    highest = Math.max(highest, Number(match[1]));
  }
  return Math.max(connected, highest + 1);
}
