import { writable } from 'svelte/store';
import { snapshot } from './store.js';
import { createHistory } from './history.js';

const history = createHistory();

export const sensorHistory = writable({ get: history.get, delta: history.delta, version: 0 });

let version = 0;
snapshot.subscribe((snap) => {
  if (!snap || !snap.sensors) return;
  history.record(snap.sensors, Date.now());
  version += 1;
  sensorHistory.set({ get: history.get, delta: history.delta, version });
});
