import { test } from 'node:test';
import assert from 'node:assert/strict';
import { shouldSnapshotEdit } from '../../src/lib/graph/snapshotDebounce.js';

test('shouldSnapshotEdit snapshots the first edit in a burst', () => {
  assert.equal(shouldSnapshotEdit(null, 1000, 400), true);
});

test('shouldSnapshotEdit skips edits that follow closely behind the last one', () => {
  assert.equal(shouldSnapshotEdit(1000, 1100, 400), false);
  assert.equal(shouldSnapshotEdit(1000, 1399, 400), false);
});

test('shouldSnapshotEdit snapshots again once the gap exceeds the window', () => {
  assert.equal(shouldSnapshotEdit(1000, 1400, 400), true);
  assert.equal(shouldSnapshotEdit(1000, 2000, 400), true);
});
