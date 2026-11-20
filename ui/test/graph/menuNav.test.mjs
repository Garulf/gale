import { test } from 'node:test';
import assert from 'node:assert/strict';
import { nextMenuIndex } from '../../src/lib/graph/menuNav.js';

test('arrows move between enabled items and wrap', () => {
  const enabled = [true, false, true, true];
  assert.equal(nextMenuIndex(enabled, 0, 'ArrowDown'), 2);
  assert.equal(nextMenuIndex(enabled, 3, 'ArrowDown'), 0);
  assert.equal(nextMenuIndex(enabled, 0, 'ArrowUp'), 3);
  assert.equal(nextMenuIndex(enabled, 2, 'ArrowUp'), 0);
});

test('Home and End jump to the first and last enabled item', () => {
  const enabled = [false, true, true, false];
  assert.equal(nextMenuIndex(enabled, 2, 'Home'), 1);
  assert.equal(nextMenuIndex(enabled, 1, 'End'), 2);
});

test('arrows from outside the menu land on an end, other keys keep the position', () => {
  const enabled = [true, true, true];
  assert.equal(nextMenuIndex(enabled, -1, 'ArrowDown'), 0);
  assert.equal(nextMenuIndex(enabled, -1, 'ArrowUp'), 2);
  assert.equal(nextMenuIndex(enabled, 1, 'Tab'), 1);
  assert.equal(nextMenuIndex([false, false], 0, 'ArrowDown'), -1);
});
