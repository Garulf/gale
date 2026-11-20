import { test } from 'node:test';
import assert from 'node:assert/strict';
import { emphasizeEdges, carryEdgeSelection } from '../../src/lib/graph/emphasis.js';
import { fitOptions, canFit, MAX_FIT_ZOOM } from '../../src/lib/graph/viewport.js';

const edges = [
  { id: 'a', source: 'sensor:x', target: 'curve:cpu', class: 'temp', data: { kind: 'temp' } },
  { id: 'b', source: 'curve:cpu', target: 'control:x', class: 'duty', data: { kind: 'duty' } },
  { id: 'c', source: 'sensor:x', target: 'curve:gpu', class: 'temp', data: { kind: 'temp' } },
];

test('selecting a node marks its edges active and dims the rest', () => {
  const marked = emphasizeEdges(edges, 'curve:cpu');
  assert.deepEqual(marked.map((edge) => edge.data.emphasis), ['active', 'active', 'dim']);
  assert.equal(marked[0].class, 'temp edge-active');
  assert.equal(marked[2].class, 'temp edge-dim');
});

test('no selection, or a selection with no edges in view, leaves every edge alone', () => {
  assert.equal(emphasizeEdges(edges, ''), edges);
  assert.equal(emphasizeEdges(edges, 'curve:elsewhere'), edges);
});

test('edge selection survives a rebuild of the edge list', () => {
  const previous = [{ ...edges[0], selected: true }, edges[1]];
  const next = carryEdgeSelection(previous, edges);
  assert.equal(next[0].selected, true);
  assert.equal(next[1].selected, undefined);
  assert.equal(carryEdgeSelection(edges, edges), edges);
});

test('fit options cap the zoom and leave room for the chrome', () => {
  const wide = fitOptions(false);
  const narrow = fitOptions(true);
  assert.equal(wide.maxZoom, MAX_FIT_ZOOM);
  assert.ok(wide.minZoom < 0.5);
  assert.equal(narrow.padding.bottom, '80px');
  assert.notDeepEqual(wide.padding, narrow.padding);
});

test('fitting waits for a measured canvas with initialized nodes', () => {
  const ready = { width: 800, height: 600, nodes: [{}], nodesInitialized: true };
  assert.equal(canFit(ready), true);
  assert.equal(canFit({ ...ready, width: 0 }), false);
  assert.equal(canFit({ ...ready, nodes: [] }), false);
  assert.equal(canFit({ ...ready, nodesInitialized: false }), false);
});
