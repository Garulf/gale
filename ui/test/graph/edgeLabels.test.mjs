import { test } from 'node:test';
import assert from 'node:assert/strict';
import { showsEdgeLabel, OVERVIEW_ZOOM } from '../../src/lib/graph/viewport.js';

test('edge labels show at working zoom', () => {
  assert.equal(showsEdgeLabel(1, false), true);
  assert.equal(showsEdgeLabel(OVERVIEW_ZOOM, false), true);
});

test('unemphasized edge labels hide at overview zoom', () => {
  assert.equal(showsEdgeLabel(0.59, false), false);
  assert.equal(showsEdgeLabel(0.3, false), false);
});

test('selected or emphasized edges keep their label at overview zoom', () => {
  assert.equal(showsEdgeLabel(0.3, true), true);
});
