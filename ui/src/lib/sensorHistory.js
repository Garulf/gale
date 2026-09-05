import { writable } from 'svelte/store';
import { snapshot } from './store.js';
import { createHistory } from './history.js';

const history = createHistory();

export const sensorHistory = writable({ get: history.get, delta: history.delta });

snapshot.subscribe((snap) => {
  if (!snap || !snap.sensors) return;
  history.record(snap.sensors, Date.now());
  sensorHistory.set({ get: history.get, delta: history.delta });
});
