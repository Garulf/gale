import {
  deviceOf,
  sensorNodeId,
  controlNodeId,
  virtualNodeId,
  curveNodeId,
  combineNodeId,
  isCombineType,
  isSingleInputCombineType,
  nodeIdForCurveRef,
  nodeKind,
  nodeName,
  edgeId,
  virtualInputHandles,
} from './ids.js';
import { VIRTUAL_PREFIX, isVirtualId, virtualId, virtualSensorInputs } from '../sensors.js';
import { tachSensorFor } from '../tach.js';
import { pruneGroups } from './groups.js';

const SENSOR_INPUT_CURVE_TYPES = ['point', 'linear', 'trigger', 'target'];

const COLUMN_X = { sensor: 40, virtual: 360, curve: 680, combine: 680, control: 1000 };
const ROW_START = 40;
const ROW_HEIGHT = 240;

function virtualNameFromId(id) {
  return id.slice(VIRTUAL_PREFIX.length);
}

function groupByDevice(items) {
  const map = new Map();
  for (const item of items) {
    const device = deviceOf(item.id);
    if (!map.has(device)) map.set(device, []);
    map.get(device).push(item);
  }
  return map;
}

function makeEdge(source, sourceHandle, target, targetHandle, kind) {
  return {
    id: edgeId(source, sourceHandle, target, targetHandle),
    source,
    sourceHandle,
    target,
    targetHandle,
    data: { kind },
  };
}

function resolveSensorSource(input) {
  if (isVirtualId(input)) {
    return { source: virtualNodeId(virtualNameFromId(input)), sourceHandle: 'out' };
  }
  return { source: sensorNodeId(deviceOf(input)), sourceHandle: input };
}

function combineInputHandles(type, count) {
  if (isSingleInputCombineType(type)) return ['in'];
  return Array.from({ length: count }, (_, i) => `in-${i}`);
}

function curveConfigWithoutWiring(curveConfig) {
  const { sensor, source, sources, ...rest } = curveConfig;
  return rest;
}

function fallbackPositions(allNodeIds) {
  const byKind = new Map();
  for (const id of allNodeIds) {
    const column = nodeKind(id) === 'combine' ? 'curve' : nodeKind(id);
    if (!byKind.has(column)) byKind.set(column, []);
    byKind.get(column).push(id);
  }
  const positions = new Map();
  for (const ids of byKind.values()) {
    ids.sort();
    ids.forEach((id, index) => {
      positions.set(id, { x: COLUMN_X[nodeKind(id)], y: ROW_START + ROW_HEIGHT * index });
    });
  }
  return positions;
}

export function withWiredRows(nodes, edges) {
  let changed = false;
  const next = nodes.map((node) => {
    if (node.type !== 'deviceSensor' && node.type !== 'deviceControl') return node;
    const key = node.type;
    let touched = false;
    const rows = node.data[key].rows.map((row) => {
      const wired =
        key === 'deviceSensor'
          ? edges.some((edge) => edge.source === node.id && edge.sourceHandle === row.handle)
          : edges.some((edge) => edge.target === node.id && edge.targetHandle === row.handle);
      if (wired === (row.wired === true)) return row;
      touched = true;
      return { ...row, wired };
    });
    if (!touched) return node;
    changed = true;
    return { ...node, data: { [key]: { ...node.data[key], rows } } };
  });
  return changed ? next : nodes;
}

function markWiredRows(nodeSpecs, edges) {
  for (const spec of nodeSpecs) {
    if (spec.type === 'deviceSensor') {
      for (const row of spec.data.deviceSensor.rows) {
        row.wired = edges.some((edge) => edge.source === spec.id && edge.sourceHandle === row.handle);
      }
    } else if (spec.type === 'deviceControl') {
      for (const row of spec.data.deviceControl.rows) {
        row.wired = edges.some((edge) => edge.target === spec.id && edge.targetHandle === row.handle);
      }
    }
  }
}

function handleIndex(handle) {
  if (handle === 'in') return 0;
  const match = /^in-(\d+)$/.exec(handle);
  return match ? Number(match[1]) : 0;
}

function readPorts(ports) {
  if (!Array.isArray(ports)) return [];
  return ports
    .filter((port) => port && typeof port.node === 'string' && typeof port.handle === 'string')
    .map((port) => ({ node: port.node, handle: port.handle, name: typeof port.name === 'string' ? port.name : null }));
}

function prunePorts(group) {
  const members = new Set(group.members);
  const keep = (port) => members.has(port.node);
  return { ...group, inputs: group.inputs.filter(keep), outputs: group.outputs.filter(keep) };
}

export function configToGraph(config, inventory, profileName) {
  const profile = config.profiles[profileName];
  const edges = [];
  const nodeSpecs = [];

  const hiddenIds = (config.ui && config.ui.hidden && config.ui.hidden[profileName]) || [];
  const compactIds = (config.ui && config.ui.compact && config.ui.compact[profileName]) || [];
  const sensorsByDevice = groupByDevice(inventory.sensors || []);
  for (const [device, rows] of sensorsByDevice) {
    nodeSpecs.push({
      id: sensorNodeId(device),
      type: 'deviceSensor',
      data: { deviceSensor: { device, rows: rows.map((row) => ({ handle: row.id, label: row.label, kind: row.kind, hidden: hiddenIds.includes(row.id) })) } },
    });
  }

  const controlsByDevice = groupByDevice(inventory.controls || []);
  for (const [device, rows] of controlsByDevice) {
    nodeSpecs.push({
      id: controlNodeId(device),
      type: 'deviceControl',
      data: {
        deviceControl: {
          device,
          rows: rows.map((row) => {
            const tach = tachSensorFor(row.id, inventory.sensors || []);
            return { handle: row.id, label: row.label, tach: tach ? tach.id : null, hidden: hiddenIds.includes(row.id) };
          }),
        },
      },
    });
  }

  for (const [name, sensorConfig] of Object.entries(profile.sensors || {})) {
    nodeSpecs.push({
      id: virtualNodeId(name),
      type: 'virtual',
      data: { virtual: { name, config: sensorConfig } },
    });

    const inputs = virtualSensorInputs(sensorConfig);
    const handles = virtualInputHandles(sensorConfig.type, inputs.length);
    inputs.forEach((input, index) => {
      const { source, sourceHandle } = resolveSensorSource(input);
      edges.push(makeEdge(source, sourceHandle, virtualNodeId(name), handles[index], 'temp'));
    });
  }

  for (const [id, curveConfig] of Object.entries(profile.curves || {})) {
    const combine = isCombineType(curveConfig.type);
    const nodeId = combine ? combineNodeId(id) : curveNodeId(id);
    nodeSpecs.push({
      id: nodeId,
      type: combine ? 'combine' : 'curve',
      data: { [combine ? 'combine' : 'curve']: { id, config: curveConfigWithoutWiring(curveConfig) } },
    });

    if (SENSOR_INPUT_CURVE_TYPES.includes(curveConfig.type)) {
      if (curveConfig.sensor) {
        const { source, sourceHandle } = resolveSensorSource(curveConfig.sensor);
        edges.push(makeEdge(source, sourceHandle, nodeId, 'sensor', 'temp'));
      }
    } else if (curveConfig.type === 'mix') {
      const handles = combineInputHandles('mix', curveConfig.sources.length);
      curveConfig.sources.forEach((curveId, index) => {
        const source = nodeIdForCurveRef(curveId, profile.curves);
        edges.push(makeEdge(source, 'out', nodeId, handles[index], 'duty'));
      });
    } else if (isSingleInputCombineType(curveConfig.type)) {
      const source = nodeIdForCurveRef(curveConfig.source, profile.curves);
      edges.push(makeEdge(source, 'out', nodeId, 'in', 'duty'));
    }
  }

  for (const [controlId, curveId] of Object.entries(profile.assignments || {})) {
    const target = controlNodeId(deviceOf(controlId));
    const source = nodeIdForCurveRef(curveId, profile.curves);
    edges.push(makeEdge(source, 'out', target, controlId, 'duty'));
  }

  markWiredRows(nodeSpecs, edges);

  const allNodeIds = nodeSpecs.map((spec) => spec.id);
  const storedPositions = (config.ui && config.ui.graph && config.ui.graph[profileName]) || {};
  const fallback = fallbackPositions(allNodeIds);
  const nodes = nodeSpecs.map((spec) => {
    const stored = storedPositions[spec.id];
    const position = stored ? { x: stored[0], y: stored[1] } : fallback.get(spec.id);
    return {
      id: spec.id,
      type: spec.type,
      position,
      hidden: hiddenIds.includes(spec.id),
      compact: compactIds.includes(spec.id),
      data: spec.data,
    };
  });

  const storedGroups = (config.ui && config.ui.groups && config.ui.groups[profileName]) || {};
  const groups = pruneGroups(
    Object.entries(storedGroups).map(([id, group]) => ({
      id,
      name: group.name || `Group ${id.replace(/^g/, '')}`,
      position: { x: (group.position || [0, 0])[0], y: (group.position || [0, 0])[1] },
      members: group.members || [],
      inputs: readPorts(group.inputs),
      outputs: readPorts(group.outputs),
      parent: group.parent || null,
    })),
    nodes
  );

  return { nodes, edges, groups: groups.map(prunePorts) };
}

function refIdForSource(edge) {
  const kind = nodeKind(edge.source);
  if (kind === 'sensor') return edge.sourceHandle;
  if (kind === 'virtual') return virtualId(nodeName(edge.source));
  return nodeName(edge.source);
}

function writePort(port) {
  return { node: port.node, handle: port.handle, ...(port.name ? { name: port.name } : {}) };
}

export function graphToConfig(nodes, edges, baseConfig, profileName, groups = []) {
  const config = structuredClone(baseConfig);
  if (!config.profiles) config.profiles = {};

  const profile = { curves: {}, assignments: {}, sensors: {} };

  for (const node of nodes) {
    if (node.type !== 'virtual') continue;
    const name = nodeName(node.id);
    const incoming = edges
      .filter((edge) => edge.target === node.id)
      .sort((a, b) => handleIndex(a.targetHandle) - handleIndex(b.targetHandle));
    const inputIds = incoming.map(refIdForSource);
    const sensorConfig = { ...node.data.virtual.config };
    if ('inputs' in sensorConfig) {
      sensorConfig.inputs = inputIds;
    } else if ('input' in sensorConfig) {
      sensorConfig.input = inputIds.length > 0 ? inputIds[0] : '';
    }
    profile.sensors[name] = sensorConfig;
  }

  for (const node of nodes) {
    if (node.type !== 'curve' && node.type !== 'combine') continue;
    const id = nodeName(node.id);
    const stored = node.data[node.type].config;
    const curveConfig = { ...stored };
    const incoming = edges.filter((edge) => edge.target === node.id);

    if (SENSOR_INPUT_CURVE_TYPES.includes(curveConfig.type)) {
      const edge = incoming.find((candidate) => candidate.targetHandle === 'sensor');
      curveConfig.sensor = edge ? refIdForSource(edge) : '';
    } else if (isSingleInputCombineType(curveConfig.type)) {
      const edge = incoming.find((candidate) => candidate.targetHandle === 'in');
      curveConfig.source = edge ? refIdForSource(edge) : '';
    } else if (curveConfig.type === 'mix') {
      const sorted = incoming
        .slice()
        .sort((a, b) => handleIndex(a.targetHandle) - handleIndex(b.targetHandle));
      curveConfig.sources = sorted.map(refIdForSource);
    }

    profile.curves[id] = curveConfig;
  }

  for (const edge of edges) {
    if (nodeKind(edge.target) !== 'control') continue;
    profile.assignments[edge.targetHandle] = refIdForSource(edge);
  }

  config.profiles[profileName] = profile;

  if (!config.ui) config.ui = { graph: {}, hidden: {} };
  if (!config.ui.graph) config.ui.graph = {};
  if (!config.ui.hidden) config.ui.hidden = {};
  if (!config.ui.compact) config.ui.compact = {};
  config.ui.compact[profileName] = nodes.filter((node) => node.compact === true).map((node) => node.id);

  const graph = {};
  for (const node of nodes) {
    graph[node.id] = [node.position.x, node.position.y];
  }
  config.ui.graph[profileName] = graph;

  const deviceNodes = nodes.filter((node) => nodeKind(node.id) === 'sensor' || nodeKind(node.id) === 'control');
  config.ui.hidden[profileName] = [
    ...deviceNodes.filter((node) => node.hidden === true).map((node) => node.id),
    ...deviceNodes.flatMap((node) => node.data[node.type].rows.filter((row) => row.hidden === true).map((row) => row.handle)),
  ];

  if (!config.ui.groups) config.ui.groups = {};
  const storedGroups = {};
  for (const group of groups) {
    const inputs = (group.inputs || []).map(writePort);
    const outputs = (group.outputs || []).map(writePort);
    storedGroups[group.id] = {
      name: group.name,
      position: [group.position.x, group.position.y],
      members: [...group.members],
      ...(inputs.length > 0 ? { inputs } : {}),
      ...(outputs.length > 0 ? { outputs } : {}),
      ...(group.parent ? { parent: group.parent } : {}),
    };
  }
  config.ui.groups[profileName] = storedGroups;

  return config;
}

export function edgeInto(edges, target, targetHandle) {
  return edges.find((edge) => edge.target === target && edge.targetHandle === targetHandle);
}

export function isSingleInputHandle(nodeType, targetHandle) {
  if (nodeType === 'curve') return targetHandle === 'sensor';
  if (nodeType === 'combine' || nodeType === 'virtual') return targetHandle === 'in';
  return false;
}

export function replaceEdge(edges, newEdge, target, targetHandle) {
  const remaining = edges.filter((edge) => !(edge.target === target && edge.targetHandle === targetHandle));
  return [...remaining, newEdge];
}
