import dagre from '@dagrejs/dagre';
import { foldRows } from './fold.js';
import { numberedInputCount, virtualInputHandles } from './ids.js';

const NODE_WIDTH = 236;
const HEAD_HEIGHT = 52;
const ROW_HEIGHT = 26;
const ROWS_PADDING = 12;
const CHART_HEIGHT = 74;

function connectedHandles(node, edges) {
  return edges.filter((edge) => edge.target === node.id).map((edge) => edge.targetHandle);
}

function rowCount(node, edges) {
  const data = node.data || {};
  if (node.type === 'deviceSensor') return foldRows(data.deviceSensor.rows).visible.length + 1;
  if (node.type === 'deviceControl') return foldRows(data.deviceControl.rows).visible.length + 1;
  if (node.type === 'virtual') {
    const handles = virtualInputHandles(data.virtual.config.type, numberedInputCount(connectedHandles(node, edges)) + 1);
    return handles.length + 1;
  }
  if (node.type === 'combine') {
    const handles = data.combine.config.type === 'sync' ? 1 : numberedInputCount(connectedHandles(node, edges)) + 1;
    return handles + 1;
  }
  return 2;
}

export function measure(node, edges) {
  const chart = node.type === 'curve' && node.data && node.data.curve.config.type === 'point' ? CHART_HEIGHT : 0;
  return { width: NODE_WIDTH, height: HEAD_HEIGHT + ROWS_PADDING + rowCount(node, edges) * ROW_HEIGHT + chart };
}

export function autoLayout(nodes, edges) {
  const graph = new dagre.graphlib.Graph();
  graph.setDefaultEdgeLabel(() => ({}));
  graph.setGraph({ rankdir: 'LR' });

  for (const node of nodes) {
    const size = measure(node, edges);
    graph.setNode(node.id, { width: size.width, height: size.height });
  }

  for (const edge of edges) {
    graph.setEdge(edge.source, edge.target);
  }

  dagre.layout(graph);

  return nodes.map((node) => {
    const laidOut = graph.node(node.id);
    return {
      ...node,
      position: { x: laidOut.x, y: laidOut.y },
    };
  });
}
