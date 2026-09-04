import dagre from '@dagrejs/dagre';

const DEFAULT_SIZE = { width: 150, height: 64 };

function measure(node) {
  if (node.type === 'curve' && node.data && node.data.curve && node.data.curve.config.type === 'point') {
    return { width: 152, height: 120 };
  }
  return DEFAULT_SIZE;
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
