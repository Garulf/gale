import dagre from '@dagrejs/dagre';
import { curvePoints } from '../curveMath.js';
import { foldRows, shownRows } from './fold.js';
import { numberedInputCount, virtualInputHandles, isSingleInputCombineType } from './ids.js';

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
  if (node.type === 'deviceSensor') return foldRows(shownRows(data.deviceSensor.rows)).visible.length + 1;
  if (node.type === 'deviceControl') return foldRows(shownRows(data.deviceControl.rows)).visible.length + 1;
  if (node.type === 'virtual') {
    const handles = virtualInputHandles(data.virtual.config.type, numberedInputCount(connectedHandles(node, edges)) + 1);
    return handles.length + 1;
  }
  if (node.type === 'combine') {
    const handles = isSingleInputCombineType(data.combine.config.type) ? 1 : numberedInputCount(connectedHandles(node, edges)) + 1;
    return handles + 1;
  }
  if (node.type === 'group') return Math.max(data.group.inputs.length, data.group.outputs.length, 1) + 1;
  if (node.type === 'port') return 2;
  return 2;
}

export function measure(node, edges) {
  const chart = node.type === 'curve' && node.data && node.compact !== true && curvePoints(node.data.curve.config) ? CHART_HEIGHT : 0;
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

  const positioned = nodes.map((node) => {
    const laidOut = graph.node(node.id);
    return {
      ...node,
      position: { x: laidOut.x, y: laidOut.y },
    };
  });
  return pinOuterColumns(positioned, edges);
}

const COLUMN_GAP = 80;
const STACK_GAP = 24;

function pinsLeft(node) {
  return node.type === 'deviceSensor' || (node.type === 'port' && node.data.port.direction === 'in');
}

function pinsRight(node) {
  return node.type === 'deviceControl' || (node.type === 'port' && node.data.port.direction === 'out');
}

function isPinned(node) {
  return pinsLeft(node) || pinsRight(node);
}

function stackColumn(column, x, top, edges) {
  let y = top;
  return column
    .slice()
    .sort((a, b) => a.position.y - b.position.y)
    .map((node) => {
      const placed = { ...node, position: { x, y } };
      y += measure(node, edges).height + STACK_GAP;
      return placed;
    });
}

function pinOuterColumns(nodes, edges) {
  const middle = nodes.filter((node) => !isPinned(node));
  const leftColumn = nodes.filter(pinsLeft);
  const rightColumn = nodes.filter(pinsRight);
  if (leftColumn.length === 0 && rightColumn.length === 0) return nodes;

  const anchor = middle.length > 0 ? middle : nodes;
  const left = Math.min(...anchor.map((node) => node.position.x));
  const right = Math.max(...anchor.map((node) => node.position.x));
  const top = Math.min(...anchor.map((node) => node.position.y));

  const leftPlaced = stackColumn(leftColumn, left - NODE_WIDTH - COLUMN_GAP, top, edges);
  const rightPlaced = stackColumn(rightColumn, right + NODE_WIDTH + COLUMN_GAP, top, edges);
  const byId = new Map([...leftPlaced, ...rightPlaced].map((node) => [node.id, node]));
  return nodes.map((node) => byId.get(node.id) || node);
}
