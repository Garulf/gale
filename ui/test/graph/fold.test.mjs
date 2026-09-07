import { test } from 'node:test';
import assert from 'node:assert/strict';
import { foldRows, shownRows } from '../../src/lib/graph/fold.js';

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

test('unwirable rpm rows are folded before temperature rows they can never be wired to', () => {
  const deviceRows = [
    { handle: 'fan1', wired: false, kind: 'rpm' },
    { handle: 'fan2', wired: false, kind: 'rpm' },
    { handle: 'temp1', wired: false, kind: 'temp' },
    { handle: 'temp2', wired: false, kind: 'temp' },
  ];
  const { visible, hidden } = foldRows(deviceRows, 2);
  assert.deepEqual(handles(visible), ['temp1', 'temp2']);
  assert.deepEqual(handles(hidden), ['fan1', 'fan2']);
});

test('a wired rpm row still counts against the visible budget, and remaining slots prefer temperature rows', () => {
  const deviceRows = [
    { handle: 'fan1', wired: false, kind: 'rpm' },
    { handle: 'fan2', wired: false, kind: 'rpm' },
    { handle: 'temp1', wired: true, kind: 'temp' },
    { handle: 'temp2', wired: false, kind: 'temp' },
  ];
  const { visible, hidden } = foldRows(deviceRows);
  assert.deepEqual(handles(visible), ['fan1', 'temp1', 'temp2']);
  assert.deepEqual(handles(hidden), ['fan2']);
});

test('a power row is treated like a temp row for folding, ahead of an unwirable rpm row', () => {
  const deviceRows = [
    { handle: 'fan1', wired: false, kind: 'rpm' },
    { handle: 'fan2', wired: false, kind: 'rpm' },
    { handle: 'power1', wired: false, kind: 'power' },
    { handle: 'temp1', wired: false, kind: 'temp' },
  ];
  const { visible, hidden } = foldRows(deviceRows, 2);
  assert.deepEqual(handles(visible), ['power1', 'temp1']);
  assert.deepEqual(handles(hidden), ['fan1', 'fan2']);
});

test('shownRows drops hidden rows unless they are wired', () => {
  const rows = [
    { handle: 'a', hidden: true, wired: false },
    { handle: 'b', hidden: true, wired: true },
    { handle: 'c', hidden: false, wired: false },
    { handle: 'd' },
  ];
  assert.deepEqual(shownRows(rows).map((row) => row.handle), ['b', 'c', 'd']);
});
