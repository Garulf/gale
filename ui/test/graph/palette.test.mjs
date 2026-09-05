import { test } from 'node:test';
import assert from 'node:assert/strict';
import { PALETTE, paletteEntryId, operationLabel } from '../../src/lib/graph/palette.js';

test('palette ids are unique and every entry names a kind and type', () => {
  const ids = PALETTE.flatMap((group) => group.entries.map(paletteEntryId));
  assert.equal(new Set(ids).size, ids.length);
  for (const entry of PALETTE.flatMap((group) => group.entries)) {
    assert.ok(['curve', 'virtual', 'combine'].includes(entry.kind), entry.label);
    assert.ok(entry.type, entry.label);
  }
  assert.ok(ids.includes('combine-mix-avg'));
  assert.ok(ids.includes('combine-offset'));
});

test('operationLabel names the operation, not the storage type', () => {
  assert.equal(operationLabel('virtual', { type: 'mean' }), 'temperature · average');
  assert.equal(operationLabel('virtual', { type: 'max' }), 'temperature · max');
  assert.equal(operationLabel('combine', { type: 'mix', mode: 'avg' }), 'duty · average');
  assert.equal(operationLabel('combine', { type: 'offset' }), 'duty · offset');
  assert.equal(operationLabel('combine', { type: 'sync' }), 'duty · sync');
  assert.equal(operationLabel('curve', { type: 'linear' }), 'linear curve');
});
