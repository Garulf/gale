export function isWebhookNotFound(err) {
  return /^404\b/.test(err.message);
}

export function webhookNameFor(node) {
  if (!node || node.type !== 'virtual') return '';
  const config = node.data.virtual.config;
  if (config.type !== 'webhook') return '';
  return node.data.virtual.name;
}
