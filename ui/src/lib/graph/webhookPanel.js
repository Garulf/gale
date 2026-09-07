export function isWebhookNotFound(err) {
  return /^404\b/.test(err.message);
}

export function webhookNameFor(node) {
  if (!node || node.type !== 'virtual') return '';
  const config = node.data.virtual.config;
  if (config.type !== 'webhook') return '';
  return node.data.virtual.name;
}

const TOKEN_TAIL = /\/api\/webhook\/[0-9a-f]+$/i;

export function maskWebhookUrl(url) {
  if (!url) return '';
  if (TOKEN_TAIL.test(url)) return url.replace(TOKEN_TAIL, '/api/webhook/' + '\u2022'.repeat(16));
  return '\u2022'.repeat(24);
}

export function webhookSensorNodes(nodes) {
  return (nodes || [])
    .filter((node) => node.type === 'virtual' && node.hidden !== true && node.data.virtual.config.type === 'webhook')
    .map((node) => ({ id: node.id, name: node.data.virtual.name }))
    .sort((a, b) => a.name.localeCompare(b.name));
}
