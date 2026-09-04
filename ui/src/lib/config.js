import { writable } from 'svelte/store';
import { getConfig } from './api.js';

export const daemonConfig = writable(null);

export async function refreshConfig() {
  try {
    daemonConfig.set(await getConfig());
  } catch (error) {
    daemonConfig.set(null);
  }
}
