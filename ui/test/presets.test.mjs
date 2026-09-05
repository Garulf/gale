import { test } from 'node:test';
import assert from 'node:assert/strict';
import { presetNameFor } from '../src/lib/presets.js';

const groups = {
  builtin: { Quiet: { type: 'point', points: [[30, 20], [85, 100]], hysteresis: null, response: null } },
  user: { pinned: { type: 'flat', duty: 35 } },
};

test('presetNameFor matches a config to a preset regardless of key order', () => {
  assert.equal(presetNameFor({ points: [[30, 20], [85, 100]], type: 'point', response: null, hysteresis: null }, groups), 'Quiet');
  assert.equal(presetNameFor({ duty: 35, type: 'flat' }, groups), 'pinned');
});

test('presetNameFor returns an empty string when nothing matches', () => {
  assert.equal(presetNameFor({ type: 'flat', duty: 36 }, groups), '');
  assert.equal(presetNameFor({ type: 'point', points: [[30, 20]], hysteresis: null, response: null }, groups), '');
});

test('presetNameFor prefers a saved preset over a built-in with the same shape', () => {
  const shared = { builtin: groups.builtin, user: { mine: { type: 'point', points: [[30, 20], [85, 100]], hysteresis: null, response: null } } };
  assert.equal(presetNameFor(shared.user.mine, shared), 'mine');
});
