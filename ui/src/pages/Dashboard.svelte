<script>
  import { onMount } from 'svelte';
  import { snapshot } from '../lib/store.js';
  import { getInventory, setControl, releaseControl } from '../lib/api.js';
  import { refreshWarnings } from '../lib/warnings.js';

  let inventory = null;
  let error = '';
  let pending = {};
  let drafts = {};

  onMount(async () => {
    try {
      inventory = await getInventory();
    } catch (err) {
      error = err.message;
    }
  });

  function groupOf(id) {
    const slash = id.indexOf('/');
    return slash === -1 ? id : id.slice(0, slash);
  }

  function unitFor(kind) {
    if (kind === 'temp') return '°C';
    if (kind === 'rpm') return 'RPM';
    if (kind === 'duty') return '%';
    return '';
  }

  function formatValue(value, kind) {
    if (value === null || value === undefined) return '—';
    if (kind === 'temp') return value.toFixed(1);
    return Math.round(value);
  }

  $: sensorGroups = groupSensors(inventory, $snapshot);
  $: controls = inventory ? inventory.controls : [];

  function groupSensors(inv, snap) {
    if (!inv) return [];
    const values = (snap && snap.sensors) || {};
    const byGroup = new Map();
    for (const sensor of inv.sensors) {
      const group = groupOf(sensor.id);
      if (!byGroup.has(group)) byGroup.set(group, []);
      byGroup.get(group).push({ ...sensor, value: values[sensor.id] ?? null });
    }
    return [...byGroup.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  }

  function dutyFor(id) {
    if (!$snapshot) return null;
    return $snapshot.duties ? $snapshot.duties[id] ?? null : null;
  }

  function isManual(id) {
    return !!($snapshot && $snapshot.manual && id in $snapshot.manual);
  }

  function draftFor(id) {
    if (drafts[id] !== undefined) return drafts[id];
    const current = dutyFor(id);
    return current === null ? 50 : Math.round(current);
  }

  function setDraft(id, value) {
    drafts = { ...drafts, [id]: Number(value) };
  }

  async function apply(id) {
    pending = { ...pending, [id]: true };
    try {
      await setControl(id, draftFor(id));
      await refreshWarnings();
    } catch (err) {
      error = err.message;
    } finally {
      pending = { ...pending, [id]: false };
    }
  }

  async function release(id) {
    pending = { ...pending, [id]: true };
    try {
      await releaseControl(id);
      const next = { ...drafts };
      delete next[id];
      drafts = next;
      await refreshWarnings();
    } catch (err) {
      error = err.message;
    } finally {
      pending = { ...pending, [id]: false };
    }
  }
</script>

<section>
  <h2>Dashboard</h2>
  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if !inventory}
    <p class="muted">Loading inventory.</p>
  {:else}
    <h3>Sensors</h3>
    <div class="groups">
      {#each sensorGroups as [group, sensors]}
        <div class="card">
          <h4>{group}</h4>
          <table>
            <tbody>
              {#each sensors as sensor}
                <tr>
                  <td class="label">{sensor.label}</td>
                  <td class="value">
                    {formatValue(sensor.value, sensor.kind)}
                    {#if sensor.value !== null && sensor.value !== undefined}
                      <span class="unit">{unitFor(sensor.kind)}</span>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/each}
    </div>

    <h3>Controls</h3>
    <div class="controls">
      {#each controls as control}
        <div class="card control">
          <div class="control-head">
            <span class="label">{control.label}</span>
            {#if isManual(control.id)}
              <span class="badge">manual</span>
            {/if}
          </div>
          <div class="control-body">
            <span class="duty">{formatValue(dutyFor(control.id), 'duty')}%</span>
            <input
              type="range"
              min="0"
              max="100"
              value={draftFor(control.id)}
              on:input={(event) => setDraft(control.id, event.target.value)}
            />
            <input
              type="number"
              min="0"
              max="100"
              value={draftFor(control.id)}
              on:input={(event) => setDraft(control.id, event.target.value)}
            />
            <button disabled={pending[control.id]} on:click={() => apply(control.id)}>
              Apply
            </button>
            <button
              class="secondary"
              disabled={pending[control.id] || !isManual(control.id)}
              on:click={() => release(control.id)}
            >
              Release
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</section>

<style>
  h3 {
    margin: 1.5rem 0 0.5rem;
    font-size: 0.95rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    opacity: 0.7;
  }

  .error {
    color: #f87171;
  }

  .muted {
    opacity: 0.6;
  }

  .groups {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 0.75rem;
  }

  .card {
    background: #1b1e24;
    border: 1px solid #2a2f38;
    border-radius: 8px;
    padding: 0.75rem 0.9rem;
  }

  .card h4 {
    margin: 0 0 0.5rem;
    text-transform: capitalize;
    font-size: 0.9rem;
    opacity: 0.85;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.9rem;
  }

  td {
    padding: 0.2rem 0;
  }

  td.label {
    opacity: 0.8;
  }

  td.value {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .unit {
    opacity: 0.55;
    margin-left: 0.2rem;
    font-size: 0.8rem;
  }

  .controls {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 0.75rem;
  }

  .control-head {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.5rem;
  }

  .badge {
    background: #164e2e;
    color: #4ade80;
    border-radius: 999px;
    font-size: 0.7rem;
    padding: 0.1rem 0.5rem;
  }

  .control-body {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .duty {
    width: 3rem;
    font-variant-numeric: tabular-nums;
  }

  .control-body input[type='range'] {
    flex: 1;
  }

  .control-body input[type='number'] {
    width: 3.5rem;
    background: #14161a;
    color: inherit;
    border: 1px solid #2a2f38;
    border-radius: 4px;
    padding: 0.2rem;
  }

  button {
    background: #2a2f38;
    color: inherit;
    border: none;
    border-radius: 4px;
    padding: 0.3rem 0.6rem;
    cursor: pointer;
  }

  button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  button.secondary {
    background: none;
    border: 1px solid #2a2f38;
  }
</style>
