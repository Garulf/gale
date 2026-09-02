import { writable } from 'svelte/store';

export const snapshot = writable(null);
export const connected = writable(false);

const MAX_BACKOFF_MS = 10000;

function apiKey() {
  try {
    return localStorage.getItem('gale_api_key') || '';
  } catch (error) {
    return '';
  }
}

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
    };

    socket.onmessage = (event) => {
      try {
        snapshot.set(JSON.parse(event.data));
      } catch (error) {
        connected.set(true);
      }
    };

    socket.onclose = () => {
      connected.set(false);
      if (closedByUs) {
        return;
      }
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
