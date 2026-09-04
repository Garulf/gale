import { test } from 'node:test';
import assert from 'node:assert/strict';
import { isWebhookNotFound, webhookNameFor } from '../../src/lib/graph/webhookPanel.js';

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
