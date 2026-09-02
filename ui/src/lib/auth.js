import { writable } from 'svelte/store';

export const unauthorized = writable(false);

export function markUnauthorized() {
  unauthorized.set(true);
}

export function saveApiKey(key) {
  try {
    localStorage.setItem('gale_api_key', key);
  } catch (error) {
    return;
  }
}

export function apiKey() {
  try {
    return localStorage.getItem('gale_api_key') || '';
  } catch (error) {
    return '';
  }
}
