import { test } from 'node:test';
import assert from 'node:assert/strict';
import { pruneDismissed } from '../src/lib/warnings.js';

test('pruneDismissed forgets dismissals whose warning has cleared', () => {
  const dismissed = ['sensor a missing', 'sensor b missing'];
  assert.deepEqual(pruneDismissed(dismissed, ['sensor b missing', 'new warning']), ['sensor b missing']);
  assert.deepEqual(pruneDismissed(dismissed, []), []);
});
