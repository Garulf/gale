import { writable } from 'svelte/store';

export const page = writable('dashboard');

export const graphFocus = writable(null);

export function openInGraph(focus) {
  graphFocus.set(focus);
  page.set('graph');
}
