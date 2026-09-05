import { markUnauthorized, apiKey } from './auth.js';

function headers(extra) {
  const base = { ...extra };
  const key = apiKey();
  if (key) {
    base['X-Api-Key'] = key;
  }
  return base;
}

async function request(path, options) {
  const response = await fetch(path, {
    ...options,
    headers: headers(options && options.headers),
  });
  if (!response.ok) {
    if (response.status === 401) {
      markUnauthorized();
    }
    const text = await response.text().catch(() => '');
    const detail = text.trim() || response.statusText;
    throw new Error(`${response.status} ${detail}`);
  }
  return response;
}

async function requestJson(path, options) {
  const response = await request(path, options);
  if (response.status === 204) {
    return null;
  }
  return response.json();
}

export function getStatus() {
  return requestJson('/api/status');
}

export function getInventory() {
  return requestJson('/api/inventory');
}

export function getConfig() {
  return requestJson('/api/config');
}

export async function getConfigToml() {
  const response = await request('/api/config.toml');
  return response.text();
}

export async function putConfig(config) {
  const response = await request('/api/config', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(config),
  });
  if (response.status === 204) {
    return null;
  }
  return response.json();
}

export function activateProfile(name) {
  return requestJson(`/api/profiles/${encodeURIComponent(name)}/activate`, {
    method: 'POST',
  });
}

function encodeIdPath(id) {
  return id.split('/').map(encodeURIComponent).join('/');
}

export function setControl(id, duty) {
  return requestJson(`/api/controls/${encodeIdPath(id)}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ duty }),
  });
}

export function releaseControl(id) {
  return requestJson(`/api/controls/${encodeIdPath(id)}`, {
    method: 'DELETE',
  });
}

export function getWarnings() {
  return requestJson('/api/warnings');
}

export function getWebhookUrl(name) {
  return requestJson(`/api/webhook-url/${encodeURIComponent(name)}`);
}
