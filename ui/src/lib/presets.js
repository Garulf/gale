import { writable } from 'svelte/store';
import { getPresets, putPreset, deletePreset } from './api.js';

const EMPTY = { builtin: {}, user: {} };

export const presets = writable(EMPTY);

export async function refreshPresets() {
  try {
    presets.set(await getPresets());
  } catch (error) {
    presets.set(EMPTY);
  }
}

export async function savePreset(name, preset) {
  await putPreset(name, preset);
  await refreshPresets();
}

export async function removePreset(name) {
  await deletePreset(name);
  await refreshPresets();
}

function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === 'object') {
    return Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonical(value[key])]));
  }
  return value;
}

function samePreset(a, b) {
  return JSON.stringify(canonical(a)) === JSON.stringify(canonical(b));
}

export function presetNameFor(config, groups) {
  for (const group of [groups.builtin, groups.user]) {
    for (const [name, preset] of Object.entries(group)) {
      if (samePreset(config, preset)) return name;
    }
  }
  return '';
}
