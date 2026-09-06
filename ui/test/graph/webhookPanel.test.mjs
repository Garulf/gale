import { test } from 'node:test';
import assert from 'node:assert/strict';
import { isWebhookNotFound, webhookNameFor, maskWebhookUrl, webhookSensorNodes } from '../../src/lib/graph/webhookPanel.js';

test('isWebhookNotFound recognizes a 404 error message', () => {
  assert.equal(isWebhookNotFound(new Error('404 not found')), true);
  assert.equal(isWebhookNotFound(new Error('404')), true);
});

test('isWebhookNotFound rejects other statuses and messages', () => {
  assert.equal(isWebhookNotFound(new Error('401 unauthorized')), false);
  assert.equal(isWebhookNotFound(new Error('failed to fetch')), false);
  assert.equal(isWebhookNotFound(new Error('40404')), false);
});

function virtualNode(name, config) {
  return { type: 'virtual', data: { virtual: { name, config } } };
}

test('webhookNameFor returns the sensor name for a webhook virtual node', () => {
  const node = virtualNode('remote', { type: 'webhook', token: '', timeout_s: null });
  assert.equal(webhookNameFor(node), 'remote');
});

test('webhookNameFor returns empty for non-webhook virtual nodes and non-virtual nodes', () => {
  assert.equal(webhookNameFor(virtualNode('hot', { type: 'max', inputs: [] })), '');
  assert.equal(webhookNameFor({ type: 'curve', data: { curve: {} } }), '');
  assert.equal(webhookNameFor(null), '');
});

test('maskWebhookUrl hides the token but keeps the host and path visible', () => {
  const token = 'a'.repeat(64);
  const masked = maskWebhookUrl(`http://gale.local:5250/api/webhook/${token}`);
  assert.ok(masked.startsWith('http://gale.local:5250/api/webhook/'));
  assert.ok(!masked.includes(token));
  assert.ok(!masked.includes('aaaa'));
});

test('maskWebhookUrl masks an unexpected shape entirely and passes through an empty value', () => {
  assert.ok(!maskWebhookUrl('http://example/whatever?token=secret').includes('secret'));
  assert.equal(maskWebhookUrl(''), '');
});

test('webhookSensorNodes lists only webhook virtual sensors, sorted by name', () => {
  const nodes = [
    { id: 'virtual:zeta', type: 'virtual', data: { virtual: { name: 'zeta', config: { type: 'webhook' } } } },
    { id: 'virtual:hot', type: 'virtual', data: { virtual: { name: 'hot', config: { type: 'max', inputs: [] } } } },
    { id: 'virtual:alpha', type: 'virtual', data: { virtual: { name: 'alpha', config: { type: 'webhook' } } } },
    { id: 'curve:cpu', type: 'curve', data: { curve: { id: 'cpu', config: { type: 'flat', duty: 1 } } } },
  ];
  assert.deepEqual(webhookSensorNodes(nodes), [
    { id: 'virtual:alpha', name: 'alpha' },
    { id: 'virtual:zeta', name: 'zeta' },
  ]);
  assert.deepEqual(webhookSensorNodes(undefined), []);
});
