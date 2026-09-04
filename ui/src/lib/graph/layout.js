import dagre from '@dagrejs/dagre';

const NODE_WIDTH = 236;
const HEAD_HEIGHT = 52;
const ROW_HEIGHT = 26;
const ROWS_PADDING = 12;
const CHART_HEIGHT = 74;

function rowCount(node) {
  const data = node.data || {};
  if (node.type === 'deviceSensor') return Math.min(3, data.deviceSensor.rows.length) + 1;
  if (node.type === 'deviceControl') return Math.min(3, data.deviceControl.rows.length) + 1;
  if (node.type === 'virtual') return (data.virtual.config.inputs ? data.virtual.config.inputs.length : 1) + 2;
  if (node.type === 'combine') return (data.combine.config.sources ? data.combine.config.sources.length : 1) + 2;
  return 2;
}

function measure(node) {
  const chart = node.type === 'curve' && node.data && node.data.curve.config.type === 'point' ? CHART_HEIGHT : 0;
  return { width: NODE_WIDTH, height: HEAD_HEIGHT + ROWS_PADDING + rowCount(node) * ROW_HEIGHT + chart };
}

export function autoLayout(nodes, edges) {
  const graph = new dagre.graphlib.Graph();
  graph.setDefaultEdgeLabel(() => ({}));
  graph.setGraph({ rankdir: 'LR' });

  for (const node of nodes) {
    const size = measure(node);
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
