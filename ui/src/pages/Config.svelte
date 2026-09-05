<script>
  import { onMount } from 'svelte';
  import { daemonConfig, configError, refreshConfig } from '../lib/config.js';
  import { snapshot } from '../lib/store.js';
  import { getInventory, activateProfile, getConfigToml } from '../lib/api.js';
  import { refreshWarnings } from '../lib/warnings.js';
  import { openInGraph } from '../lib/page.js';
  import { sensorLabel } from '../lib/sensors.js';
  import { isCombineType, nodeIdForCurveRef } from '../lib/graph/ids.js';

  let inventory = $state(null);
  let toml = $state('');
  let error = $state('');
  let busy = $state('');

  onMount(load);

  async function load() {
    refreshConfig();
    try {
      [inventory, toml] = await Promise.all([getInventory(), getConfigToml()]);
    } catch (err) {
      error = err.message;
    }
  }

  let config = $derived($daemonConfig);
  let activeName = $derived($snapshot ? $snapshot.active_profile : config ? config.active_profile : '');
  let profile = $derived(config ? config.profiles[activeName] || null : null);

  function corsairReleaseLabel(onRelease) {
    if (onRelease === 'pin_full') return 'Pin to 100%';
    if (onRelease === 'keep_last') return 'Keep last duty';
    if (onRelease && typeof onRelease === 'object' && 'fixed' in onRelease) {
      return `Fixed at ${onRelease.fixed}%`;
    }
    return onRelease ? JSON.stringify(onRelease) : 'Keep last duty';
  }

  let profiles = $derived.by(() => {
    if (!config) return [];
    return Object.keys(config.profiles)
      .sort()
      .map((name) => {
        const entry = config.profiles[name];
        return {
          name,
          active: name === activeName,
          curves: Object.keys(entry.curves || {}).length,
          sensors: Object.keys(entry.sensors || {}).length,
        };
      });
  });

  function controlLabel(id) {
    const control = inventory ? inventory.controls.find((entry) => entry.id === id) : null;
    return control ? control.label : id;
  }

  function detailOf(curve) {
    switch (curve.type) {
      case 'point':
        return (curve.points || []).map(([temp, duty]) => `${temp}→${duty}`).join('  ');
      case 'flat':
        return `${curve.duty}%`;
      case 'linear':
        return `${curve.min_temp}°→${curve.min_duty}%  ${curve.max_temp}°→${curve.max_duty}%`;
      case 'trigger':
        return `on ${curve.on_temp}°→${curve.on_duty}%  off ${curve.off_temp}°→${curve.off_duty}%`;
      case 'target':
        return `target ${curve.target_temp}°  ${curve.min_duty}–${curve.max_duty}%`;
      case 'mix':
        return `mode ${curve.mode}`;
      case 'sync':
        return 'follows source';
      default:
        return '';
    }
  }

  let curves = $derived.by(() => {
    if (!profile) return [];
    const assignments = profile.assignments || {};
    return Object.entries(profile.curves || {})
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([id, curve]) => ({
        id,
        type: curve.type,
        input: isCombineType(curve.type)
          ? curve.type === 'sync'
            ? curve.source || '—'
            : (curve.sources || []).join(' + ') || '—'
          : sensorLabel(curve.sensor, inventory ? inventory.sensors : []) || '—',
        drives: Object.entries(assignments)
          .filter(([, curveId]) => curveId === id)
          .map(([controlId]) => controlLabel(controlId))
          .join(', ') || '—',
        detail: detailOf(curve),
        nodeId: nodeIdForCurveRef(id, profile.curves),
      }));
  });

  async function activate(name) {
    busy = name;
    error = '';
    try {
      await activateProfile(name);
      await Promise.all([refreshConfig(), refreshWarnings(), getConfigToml().then((text) => (toml = text))]);
    } catch (err) {
      error = err.message;
    } finally {
      busy = '';
    }
  }
</script>

<main class="page config">
  <div class="page-title">
    <span class="eyebrow">Daemon</span>
    <h1>Configuration</h1>
  </div>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if !config}
    {#if $configError}
      <p class="error">Could not load the daemon configuration: {$configError}</p>
      <button type="button" class="btn" onclick={load}>Retry</button>
    {:else}
      <p class="muted">Loading configuration.</p>
    {/if}
  {:else}
    <section class="two-up">
      <div class="card block">
        <div class="section-head"><h2 class="section-title">Daemon settings</h2><span class="hint">restart required</span></div>
        <div class="settings">
          <span class="k">Tick interval</span><span class="mono">{config.tick_interval_ms} ms</span>
          <span class="k">API bind</span><span class="mono">{config.api.bind}</span>
          <span class="k">API key</span><span class="faint">managed via config file</span>
          <span class="k">Corsair on release</span><span>{corsairReleaseLabel(config.hardware?.corsair?.on_release)}</span>
          <span class="k">Active profile</span><span class="strong">{activeName}</span>
        </div>
      </div>
      <div class="card block">
        <div class="section-head"><h2 class="section-title">Profiles</h2><span class="hint">switching applies immediately</span></div>
        <div class="profiles">
          {#each profiles as entry (entry.name)}
            <div class="profile" class:active={entry.active}>
              <span class="dot"></span>
              <span class="name">{entry.name}</span>
              <span class="mono meta">{entry.curves} curves · {entry.sensors} virtual</span>
              {#if entry.active}
                <span class="active-tag">ACTIVE</span>
              {:else}
                <button type="button" class="btn" disabled={busy !== ''} onclick={() => activate(entry.name)}>Activate</button>
              {/if}
            </div>
          {/each}
        </div>
      </div>
    </section>

    <section class="card block">
      <div class="section-head"><h2 class="section-title">Curves in {activeName}</h2><span class="hint">click a row to open it in the graph</span></div>
      <div class="table-wrap">
        <table class="curves">
          <thead>
            <tr><th>name</th><th>type</th><th>input</th><th>drives</th><th>detail</th></tr>
          </thead>
          <tbody>
            {#each curves as curve (curve.id)}
              <tr onclick={() => openInGraph({ nodeId: curve.nodeId })}>
                <td class="strong">{curve.id}</td>
                <td class="muted">{curve.type}</td>
                <td class="temp-ink">{curve.input}</td>
                <td class="duty-ink">{curve.drives}</td>
                <td class="mono detail">{curve.detail}</td>
              </tr>
            {/each}
            {#if curves.length === 0}
              <tr><td colspan="5" class="muted">No curves in this profile yet.</td></tr>
            {/if}
          </tbody>
        </table>
      </div>
    </section>

    <section class="card block">
      <div class="section-head"><h2 class="section-title">config.toml</h2><span class="hint mono">as the daemon holds it</span></div>
      <pre class="mono">{toml}</pre>
    </section>
  {/if}
</main>

<style>
  .config {
    max-width: 1100px;
  }

  .two-up {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  .block {
    padding: 18px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .section-head {
    justify-content: space-between;
  }

  .settings {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 10px 24px;
    font-size: 13px;
    align-items: center;
  }

  .k {
    color: var(--muted);
    white-space: nowrap;
  }

  .faint {
    color: var(--faint);
  }

  .strong {
    font-weight: 600;
  }

  .profiles {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .profile {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: var(--r);
    background: var(--surface2);
  }

  .profile.active {
    border-color: var(--accent);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--faint);
  }

  .active .dot {
    background: var(--accent);
  }

  .name {
    font-weight: 600;
    flex: 1;
  }

  .meta {
    font-size: 11px;
    color: var(--muted);
  }

  .active-tag {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--accent);
  }

  .table-wrap {
    overflow-x: auto;
  }

  .curves {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }

  .curves th {
    text-align: left;
    padding: 6px 10px;
    font-size: 10px;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--faint);
    border-bottom: 1px solid var(--line);
  }

  .curves td {
    padding: 10px;
    border-bottom: 1px solid var(--line);
    vertical-align: top;
  }

  .curves tbody tr {
    cursor: pointer;
  }

  .curves tbody tr:hover td {
    background: var(--surface2);
  }

  .temp-ink {
    color: var(--temp);
  }

  .duty-ink {
    color: var(--duty);
  }

  .detail {
    font-size: 11px;
    color: var(--muted);
    white-space: pre;
  }

  pre {
    margin: 0;
    background: var(--canvas);
    border: 1px solid var(--line);
    border-radius: var(--r);
    padding: 14px;
    font-size: 12px;
    line-height: 1.6;
    overflow-x: auto;
  }

  @media (max-width: 720px) {
    .two-up {
      grid-template-columns: 1fr;
    }

    .meta {
      display: none;
    }
  }
</style>
