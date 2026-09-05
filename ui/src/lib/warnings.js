import { writable, get } from 'svelte/store';
import { getWarnings } from './api.js';

export const warnings = writable([]);
export const dismissedWarnings = writable([]);

export function pruneDismissed(dismissed, current) {
  return dismissed.filter((warning) => current.includes(warning));
}

export function dismissWarning(warning) {
  dismissedWarnings.update((dismissed) => [...dismissed, warning]);
}

export async function refreshWarnings() {
  let current = [];
  try {
    const result = await getWarnings();
    current = (result && result.warnings) || [];
  } catch (error) {
    current = [];
  }
  warnings.set(current);
  dismissedWarnings.set(pruneDismissed(get(dismissedWarnings), current));
}
