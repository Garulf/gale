import { test } from 'node:test';
import assert from 'node:assert/strict';
import { revealViewport, canvasInset, fitOptions, MIN_ZOOM } from '../../src/lib/graph/viewport.js';
import { connectedNodeIds } from '../../src/lib/graph/emphasis.js';

const inset = { top: 0, right: 0, bottom: 0, left: 0 };
const size = { width: 890, height: 800 };

test('a selection that already fits keeps the viewport', () => {
  const viewport = { x: 0, y: 0, zoom: 1 };
  assert.equal(revealViewport({ x: 100, y: 100, width: 300, height: 200 }, viewport, size, inset), null);
});

test('a selection pushed past the right edge pans left by the overflow at the same zoom', () => {
  const viewport = { x: 10, y: 20, zoom: 0.89 };
  const bounds = { x: 600, y: 100, width: 500, height: 200 };
  const next = revealViewport(bounds, viewport, size, inset);
  assert.equal(next.zoom, 0.89);
  assert.equal(next.y, 20);
  const right = (bounds.x + bounds.width) * next.zoom + next.x;
  assert.ok(Math.abs(right - size.width) < 1e-9);
});

test('a selection above the top edge pans down to the inset', () => {
  const pad = canvasInset(false);
  const next = revealViewport({ x: 100, y: -200, width: 100, height: 100 }, { x: 0, y: 0, zoom: 1 }, size, pad);
  assert.equal(next.y, pad.top + 200);
  assert.equal(next.x, 0);
});

test('a selection too big for the canvas zooms out and centres, never zooming in', () => {
  const viewport = { x: 0, y: 0, zoom: 1 };
  const bounds = { x: 0, y: 0, width: 1780, height: 400 };
  const next = revealViewport(bounds, viewport, size, inset);
  assert.equal(next.zoom, 0.5);
  assert.equal(next.x, 0);
  assert.equal(next.y, (800 - 200) / 2);
  const tiny = revealViewport({ x: 0, y: 0, width: 1e6, height: 10 }, viewport, size, inset);
  assert.equal(tiny.zoom, MIN_ZOOM);
});

test('an unmeasured canvas or selection is left alone', () => {
  assert.equal(revealViewport({ x: 0, y: 0, width: 0, height: 0 }, { x: 0, y: 0, zoom: 1 }, size, inset), null);
  assert.equal(revealViewport({ x: 0, y: 0, width: 10, height: 10 }, { x: 0, y: 0, zoom: 1 }, { width: 0, height: 0 }, inset), null);
});

test('fit padding and the reveal inset describe the same chrome', () => {
  const pad = canvasInset(false);
  assert.equal(fitOptions(false).padding.top, `${pad.top}px`);
  assert.equal(fitOptions(true).padding.bottom, `${canvasInset(true).bottom}px`);
});

test('connected ids are the node and both ends of its edges, once each', () => {
  const edges = [
    { source: 'sensor:x', target: 'curve:cpu' },
    { source: 'curve:cpu', target: 'control:x' },
    { source: 'curve:cpu', target: 'control:x' },
    { source: 'sensor:x', target: 'curve:gpu' },
  ];
  assert.deepEqual(connectedNodeIds(edges, 'curve:cpu'), ['curve:cpu', 'sensor:x', 'control:x']);
  assert.deepEqual(connectedNodeIds(edges, 'curve:lonely'), ['curve:lonely']);
  assert.deepEqual(connectedNodeIds(edges, ''), []);
});
