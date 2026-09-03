import { test } from 'node:test';
import assert from 'node:assert/strict';
import { splitLinks } from '../src/lib/linkify.js';

test('plain text is one segment', () => {
  assert.deepEqual(splitLinks('Corsair Commander Core not found'), [
    { text: 'Corsair Commander Core not found' },
  ]);
});

test('a link in the middle of a sentence keeps the trailing period in text', () => {
  assert.deepEqual(splitLinks('see https://pawnio.eu and restart.'), [
    { text: 'see ' },
    { href: 'https://pawnio.eu' },
    { text: ' and restart.' },
  ]);
});

test('two links are both split out', () => {
  assert.deepEqual(splitLinks('https://a.example and https://b.example both'), [
    { href: 'https://a.example' },
    { text: ' and ' },
    { href: 'https://b.example' },
    { text: ' both' },
  ]);
});

test('a link at the end with a trailing period keeps the period in text', () => {
  assert.deepEqual(splitLinks('install from https://pawnio.eu.'), [
    { text: 'install from ' },
    { href: 'https://pawnio.eu' },
    { text: '.' },
  ]);
});

test('a link at the end with a trailing comma keeps the comma in text', () => {
  assert.deepEqual(splitLinks('install from https://pawnio.eu, then restart'), [
    { text: 'install from ' },
    { href: 'https://pawnio.eu' },
    { text: ', then restart' },
  ]);
});
