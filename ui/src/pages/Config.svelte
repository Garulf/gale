<script>
  import { onMount } from 'svelte';
  import { daemonConfig, configError, refreshConfig } from '../lib/config.js';
  import { snapshot } from '../lib/store.js';
  import { getInventory, activateProfile, getConfigToml } from '../lib/api.js';
  import { refreshWarnings } from '../lib/warnings.js';
  import { openInGraph, page } from '../lib/page.js';
  import { sensorLabel } from '../lib/sensors.js';
  import { inertOutside } from '../lib/graph/inert.js';
  import { isCombineType, nodeIdForCurveRef } from '../lib/graph/ids.js';

  let inventory = $state(null);
  let toml = $state('');
  let error = $state('');
  let busy = $state('');

  let copied = $state(false);
  let copiedTimer;

  let expanded = $state(false);
  let expandButton = $state();
  let overlay = $state();
  let closeButton = $state();

  $effect(() => {
    if (!expanded || !overlay) return;
    const restore = inertOutside(overlay);
    closeButton?.focus();
    return restore;
  });

  function closeExpanded() {
    expanded = false;
    expandButton?.focus();
  }

  function overlayKeydown(event) {
    if (event.key !== 'Escape') return;
    event.stopPropagation();
    closeExpanded();
  }

  onMount(() => {
    load();
    return () => clearTimeout(copiedTimer);
  });

  async function copyToml() {
    try {
      await navigator.clipboard.writeText(toml);
    } catch (err) {
      error = 'Could not copy to the clipboard.';
      return;
    }
    copied = true;
    clearTimeout(copiedTimer);
    copiedTimer = setTimeout(() => (copied = false), 1500);
  }

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
      case 'offset':
        return `× ${curve.scale} + ${curve.add}%`;
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
          ? curve.type === 'sync' || curve.type === 'offset'
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

<main class="page">
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
    <div class="layout">
      <div class="primary">
        <section class="two-up">
          <div class="card block">
            <div class="section-head"><h2 class="section-title">Daemon settings</h2><span class="hint">restart required</span></div>
            <div class="settings">
              <span class="k">Tick interval</span><span class="mono">{config.tick_interval_ms} ms</span>
              <span class="k">API bind</span><span class="mono">{config.api.bind}</span>
              <span class="k">API key</span><span class="faint">managed via config file</span>
              <span class="k">Home Assistant</span><span class="mono">{config.mqtt && config.mqtt.enabled ? `MQTT on ${config.mqtt.host}:${config.mqtt.port}` : 'off'}</span>
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
          <div class="section-head"><h2 class="section-title">Curves in {activeName}</h2>{#if curves.length > 0}<span class="hint">select a curve to open it in the graph</span>{/if}</div>
          {#if curves.length === 0}
            <div class="empty-state">
              <p class="muted">No curves in this profile yet. Fans stay under hardware control until a curve drives them.</p>
              <button type="button" class="btn primary" data-testid="config-add-curve" onclick={() => page.set('graph')}>Add a curve in Graph</button>
            </div>
          {:else}
            <div class="table-wrap">
              <table class="curves">
                <thead>
                  <tr><th>name</th><th>type</th><th>input</th><th>drives</th><th>detail</th></tr>
                </thead>
                <tbody>
                  {#each curves as curve (curve.id)}
                    <tr onclick={() => openInGraph({ nodeId: curve.nodeId })}>
                      <td class="strong">
                        <button type="button" class="open-curve" aria-label="Open curve {curve.id} in graph" onclick={(event) => { event.stopPropagation(); openInGraph({ nodeId: curve.nodeId }); }}>{curve.id}<span aria-hidden="true"> ›</span></button>
                      </td>
                      <td class="muted">{curve.type}</td>
                      <td class="temp-ink">{curve.input}</td>
                      <td class="duty-ink">{curve.drives}</td>
                      <td class="mono detail">{curve.detail}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
            <ul class="curve-cards">
              {#each curves as curve (curve.id)}
                <li class="curve-card">
                  <div class="card-top">
                    <button type="button" class="open-curve strong" aria-label="Open curve {curve.id} in graph" onclick={() => openInGraph({ nodeId: curve.nodeId })}>{curve.id}<span aria-hidden="true"> ›</span></button>
                    <span class="muted type">{curve.type}</span>
                  </div>
                  <div class="chain">
                    <span class="temp-ink">{curve.input}</span>
                    <span class="muted">→ {curve.type} →</span>
                    <span class="duty-ink">{curve.drives}</span>
                  </div>
                  {#if curve.detail}
                    <details>
                      <summary>Details</summary>
                      <span class="mono detail">{curve.detail}</span>
                    </details>
                  {/if}
                </li>
              {/each}
            </ul>
          {/if}
        </section>
      </div>

      <section class="card block toml">
        <div class="section-head">
          <h2 class="section-title">config.toml</h2>
          <span class="head-actions">
            <span class="hint mono">as the daemon holds it</span>
            <button type="button" class="btn expand" data-testid="config-expand" bind:this={expandButton} onclick={() => (expanded = true)}>Expand</button>
            <button type="button" class="btn copy" onclick={copyToml}>{copied ? 'Copied' : 'Copy'}</button>
            <span class="sr-only" role="status">{copied ? 'Copied to clipboard' : ''}</span>
          </span>
        </div>
        <pre class="mono">{toml}</pre>
      </section>
    </div>
  {/if}
</main>

{#if expanded}
  <div class="scrim" role="presentation">
    <div class="overlay card" role="dialog" aria-modal="true" aria-labelledby="toml-overlay-title" tabindex="-1" bind:this={overlay} onkeydown={overlayKeydown}>
      <div class="section-head">
        <h2 class="section-title" id="toml-overlay-title">config.toml</h2>
        <span class="head-actions">
          <button type="button" class="btn copy" onclick={copyToml}>{copied ? 'Copied' : 'Copy'}</button>
          <button type="button" class="btn" data-testid="config-expand-close" bind:this={closeButton} onclick={closeExpanded}>Close</button>
        </span>
      </div>
      <pre class="mono">{toml}</pre>
    </div>
  </div>
{/if}

<style>
  .layout,
  .primary {
    display: flex;
    flex-direction: column;
    gap: 1.75rem;
    min-width: 0;
  }

  @media (min-width: 1600px) {
    .layout {
      display: grid;
      grid-template-columns: minmax(0, 3fr) minmax(0, 2fr);
      align-items: start;
    }

    .toml {
      position: sticky;
      top: 1rem;
    }

    .toml pre {
      max-height: calc(100dvh - 14.5rem);
      overflow: auto;
    }
  }

  .head-actions {
    display: inline-flex;
    align-items: center;
    gap: 0.75rem;
  }

  .copy {
    min-width: 4.5rem;
  }

  .scrim {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    background: rgb(0 0 0 / 0.55);
  }

  .overlay {
    width: min(92vw, 72rem);
    height: 90dvh;
    padding: 1.125rem;
    display: flex;
    flex-direction: column;
    gap: 0.875rem;
    min-height: 0;
  }

  .overlay pre {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.75rem;
    padding: 0.5rem 0;
  }

  .empty-state p {
    margin: 0;
    font-size: 0.8125rem;
  }

  .open-curve {
    padding: 0.25rem 0;
    border: none;
    background: none;
    color: inherit;
    font: inherit;
    font-weight: 600;
    text-align: left;
    cursor: pointer;
  }

  .open-curve:hover {
    text-decoration: underline;
  }

  .curve-cards {
    display: none;
    list-style: none;
    margin: 0;
    padding: 0;
    flex-direction: column;
    gap: 0.5rem;
  }

  .curve-card {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
    padding: 0.75rem;
    border: 1px solid var(--line);
    border-radius: var(--r);
    background: var(--surface2);
    font-size: 0.8125rem;
  }

  .card-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
  }

  .card-top .open-curve {
    min-height: 2.75rem;
    display: inline-flex;
    align-items: center;
  }

  .type {
    font-size: 0.75rem;
  }

  .chain {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
    overflow-wrap: anywhere;
  }

  .curve-card summary {
    color: var(--muted);
    font-size: 0.75rem;
    cursor: pointer;
    padding: 0.25rem 0;
  }

  .curve-card .detail {
    display: block;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .two-up {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.75rem;
  }

  .block {
    padding: 1.125rem;
    display: flex;
    flex-direction: column;
    gap: 0.875rem;
  }

  .section-head {
    justify-content: space-between;
  }

  .settings {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.625rem 1.5rem;
    font-size: 0.8125rem;
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
    gap: 0.375rem;
  }

  .profile {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.625rem 0.75rem;
    border: 1px solid var(--line);
    border-radius: var(--r);
    background: var(--surface2);
  }

  .profile.active {
    border-color: var(--accent);
  }

  .dot {
    width: 0.5rem;
    height: 0.5rem;
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
    font-size: 0.6875rem;
    color: var(--muted);
  }

  .active-tag {
    font-size: 0.625rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--accent-text);
  }

  .table-wrap {
    overflow-x: auto;
  }

  .curves {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.8125rem;
  }

  .curves th {
    text-align: left;
    padding: 0.375rem 0.625rem;
    font-size: 0.625rem;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--faint);
    border-bottom: 1px solid var(--line);
  }

  .curves td {
    padding: 0.625rem;
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
    font-size: 0.6875rem;
    color: var(--muted);
    white-space: pre;
  }

  pre {
    margin: 0;
    background: var(--canvas);
    border: 1px solid var(--line);
    border-radius: var(--r);
    padding: 0.875rem;
    font-size: 0.75rem;
    line-height: 1.6;
    overflow-x: auto;
  }

  @media (max-width: 720px) {
    .expand {
      display: none;
    }

    .two-up {
      grid-template-columns: 1fr;
    }

    .meta {
      display: none;
    }

    .table-wrap {
      display: none;
    }

    .curve-cards {
      display: flex;
    }
  }
</style>
