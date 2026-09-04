import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHistory } from '../src/lib/history.js';

test('history keeps one sample per step and drops samples outside the window', () => {
  const history = createHistory(1000, 100);
  history.record({ a: 1 }, 0);
  history.record({ a: 2 }, 50);
  history.record({ a: 3 }, 100);
  assert.deepEqual(history.get('a'), [1, 3]);
  history.record({ a: 4 }, 1100);
  assert.deepEqual(history.get('a'), [3, 4]);
});

test('history ignores null readings and reports the delta over the window', () => {
  const history = createHistory(1000, 100);
  history.record({ a: null, b: 10 }, 0);
  history.record({ b: 12.5 }, 200);
  assert.deepEqual(history.get('a'), []);
  assert.equal(history.delta('a'), 0);
  assert.equal(history.delta('b'), 2.5);
});
