import { test } from 'node:test';
import assert from 'node:assert/strict';
import { foldRows } from '../../src/lib/graph/fold.js';

function rows(wiredIndexes, count = 6) {
  return Array.from({ length: count }, (_, i) => ({ handle: `h${i}`, wired: wiredIndexes.includes(i) }));
}

const handles = (list) => list.map((row) => row.handle);

test('an all-unwired device shows the first three rows and folds the rest', () => {
  const { visible, hidden } = foldRows(rows([]));
  assert.deepEqual(handles(visible), ['h0', 'h1', 'h2']);
  assert.deepEqual(handles(hidden), ['h3', 'h4', 'h5']);
});

test('a wired row past the fold is always visible and displaces an unwired one', () => {
  const { visible, hidden } = foldRows(rows([4]));
  assert.deepEqual(handles(visible), ['h0', 'h1', 'h4']);
  assert.deepEqual(handles(hidden), ['h2', 'h3', 'h5']);
});

test('more wired rows than the initial count are all visible', () => {
  const { visible, hidden } = foldRows(rows([0, 2, 3, 5]));
  assert.deepEqual(handles(visible), ['h0', 'h2', 'h3', 'h5']);
  assert.deepEqual(handles(hidden), ['h1', 'h4']);
});

test('a device with three or fewer rows never folds', () => {
  const { visible, hidden } = foldRows(rows([], 3));
  assert.equal(visible.length, 3);
  assert.equal(hidden.length, 0);
});
