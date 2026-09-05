import { writable } from 'svelte/store';

const STORAGE_KEY = 'gale.theme';
export const THEMES = ['dark', 'light'];

function readStored() {
  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    return THEMES.includes(stored) ? stored : 'dark';
  } catch (error) {
    return 'dark';
  }
}

export const theme = writable(readStored());

theme.subscribe((value) => {
  document.documentElement.dataset.theme = value;
  try {
    window.localStorage.setItem(STORAGE_KEY, value);
  } catch (error) {
    return;
  }
});

export function toggleTheme() {
  theme.update((value) => (value === 'dark' ? 'light' : 'dark'));
}
