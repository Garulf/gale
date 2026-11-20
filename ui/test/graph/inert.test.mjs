import { test } from 'node:test';
import assert from 'node:assert/strict';
import { inertOutside } from '../../src/lib/graph/inert.js';

function element(name, children = []) {
  const node = { name, inert: false, parentElement: null, children };
  for (const child of children) child.parentElement = node;
  return node;
}

test('everything outside the dialog becomes inert and is restored, ancestors stay live', () => {
  const sheet = element('sheet');
  const canvas = element('canvas');
  const layer = element('layer', [sheet]);
  const section = element('section', [canvas, layer]);
  const nav = element('nav');
  const alreadyInert = element('toast');
  alreadyInert.inert = true;
  const body = element('body', [nav, section, alreadyInert]);

  const restore = inertOutside(sheet);
  assert.equal(canvas.inert, true);
  assert.equal(nav.inert, true);
  assert.equal(section.inert, false);
  assert.equal(layer.inert, false);
  assert.equal(sheet.inert, false);
  assert.equal(body.inert, false);

  restore();
  assert.equal(canvas.inert, false);
  assert.equal(nav.inert, false);
  assert.equal(alreadyInert.inert, true);
});
