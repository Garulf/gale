export function emphasizeEdges(edges, nodeId) {
  if (!nodeId || !edges.some((edge) => edge.source === nodeId || edge.target === nodeId)) return edges;
  return edges.map((edge) => {
    const emphasis = edge.source === nodeId || edge.target === nodeId ? 'active' : 'dim';
    return { ...edge, class: `${edge.class || ''} edge-${emphasis}`.trim(), data: { ...edge.data, emphasis } };
  });
}

export function carryEdgeSelection(previous, next) {
  const selected = new Set(previous.filter((edge) => edge.selected === true).map((edge) => edge.id));
  if (selected.size === 0) return next;
  return next.map((edge) => (selected.has(edge.id) ? { ...edge, selected: true } : edge));
}

export function connectedNodeIds(edges, nodeId) {
  if (!nodeId) return [];
  const ids = new Set([nodeId]);
  for (const edge of edges) {
    if (edge.source === nodeId) ids.add(edge.target);
    if (edge.target === nodeId) ids.add(edge.source);
  }
  return [...ids];
}
