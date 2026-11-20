import { writable } from 'svelte/store';
import { getStatus } from './api.js';
import { apiKey } from './auth.js';

export const snapshot = writable(null);
export const snapshotAt = writable(null);
export const connected = writable(false);
export const connectionLost = writable(false);

const MAX_BACKOFF_MS = 10000;

function wsUrl() {
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const key = apiKey();
  const query = key ? `?api_key=${encodeURIComponent(key)}` : '';
  return `${protocol}//${window.location.host}/api/ws${query}`;
}

export function connect() {
  let backoff = 1000;
  let socket = null;
  let closedByUs = false;

  function open() {
    socket = new WebSocket(wsUrl());

    socket.onopen = () => {
      backoff = 1000;
      connected.set(true);
      connectionLost.set(false);
    };

    socket.onmessage = (event) => {
      try {
        const parsed = JSON.parse(event.data);
        snapshotAt.set(Date.now());
        snapshot.set(parsed);
      } catch (error) {
        connected.set(true);
      }
    };

    socket.onclose = () => {
      connected.set(false);
      if (closedByUs) {
        return;
      }
      connectionLost.set(true);
      getStatus().catch(() => {});
      setTimeout(open, backoff);
      backoff = Math.min(backoff * 2, MAX_BACKOFF_MS);
    };

    socket.onerror = () => {
      socket.close();
    };
  }

  open();

  return () => {
    closedByUs = true;
    if (socket) {
      socket.close();
    }
  };
}
