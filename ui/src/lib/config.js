import { writable } from 'svelte/store';
import { getConfig } from './api.js';

export const daemonConfig = writable(null);
export const configError = writable('');

export async function refreshConfig() {
  try {
    daemonConfig.set(await getConfig());
    configError.set('');
  } catch (error) {
    daemonConfig.set(null);
    configError.set(error.message);
  }
}
