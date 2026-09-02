import { writable } from 'svelte/store';
import { getWarnings } from './api.js';

export const warnings = writable([]);
export const warningsDismissed = writable(false);

export async function refreshWarnings() {
  try {
    const result = await getWarnings();
    warnings.set((result && result.warnings) || []);
  } catch (error) {
    warnings.set([]);
  }
  warningsDismissed.set(false);
}
