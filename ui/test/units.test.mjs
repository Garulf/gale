import { test } from 'node:test';
import assert from 'node:assert/strict';
import { KIND_UNITS, axisMark } from '../src/lib/units.js';

test('axisMark shortens the degree unit and leaves the others alone', () => {
  assert.equal(axisMark(KIND_UNITS.temp), '°');
  assert.equal(axisMark(KIND_UNITS.clock), 'MHz');
  assert.equal(axisMark(KIND_UNITS.state), '');
  assert.equal(axisMark(undefined), '');
});
