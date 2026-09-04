<script>
  import { onMount } from 'svelte';
  import { getConfig } from '../lib/api.js';

  let config = $state(null);
  let error = $state('');

  onMount(async () => {
    try {
      config = await getConfig();
    } catch (err) {
      error = err.message;
    }
  });

  function pretty(value) {
    return JSON.stringify(value, null, 2);
  }

  function corsairReleaseLabel(onRelease) {
    if (onRelease === 'pin_full') return 'Pin to 100%';
    if (onRelease === 'keep_last') return 'Keep last duty';
    if (onRelease && typeof onRelease === 'object' && 'fixed' in onRelease) {
      return `Fixed at ${onRelease.fixed}%`;
    }
    return JSON.stringify(onRelease);
  }
</script>

<section>
  <h2>Config</h2>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if !config}
    <p class="muted">Loading configuration.</p>
  {:else}
    <div class="card">
      <h3>Daemon settings</h3>
      <p class="note">Restart required to change these values.</p>
      <table>
        <tbody>
          <tr>
            <td class="label">Tick interval</td>
            <td>{config.tick_interval_ms} ms</td>
          </tr>
          <tr>
            <td class="label">Corsair release</td>
            <td>{corsairReleaseLabel(config.hardware?.corsair?.on_release)}</td>
          </tr>
          <tr>
            <td class="label">API bind</td>
            <td>{config.api.bind}</td>
          </tr>
          <tr>
            <td class="label">API key</td>
            <td>managed via config file</td>
          </tr>
          <tr>
            <td class="label">Active profile</td>
            <td>{config.active_profile}</td>
          </tr>
        </tbody>
      </table>
    </div>

    <h3>Profiles</h3>
    {#each Object.keys(config.profiles).sort() as name}
      <details class="card" open={name === config.active_profile}>
        <summary>{name}{name === config.active_profile ? ' (active)' : ''}</summary>
        <details class="nested">
          <summary>Curves</summary>
          <pre>{pretty(config.profiles[name].curves)}</pre>
        </details>
        <details class="nested">
          <summary>Assignments</summary>
          <pre>{pretty(config.profiles[name].assignments)}</pre>
        </details>
      </details>
    {/each}
  {/if}
</section>

<style>
  .error {
    color: #f87171;
  }

  .muted {
    opacity: 0.6;
  }

  .card {
    background: #1b1e24;
    border: 1px solid #2a2f38;
    border-radius: 8px;
    padding: 0.75rem 1rem;
    margin-bottom: 0.75rem;
  }

  .note {
    opacity: 0.6;
    font-size: 0.85rem;
    margin-top: -0.3rem;
  }

  table {
    border-collapse: collapse;
  }

  td {
    padding: 0.2rem 0.75rem 0.2rem 0;
  }

  td.label {
    opacity: 0.7;
  }

  summary {
    cursor: pointer;
    font-weight: 600;
  }

  .nested {
    margin: 0.5rem 0 0 0.75rem;
  }

  .nested summary {
    font-weight: normal;
    opacity: 0.85;
  }

  pre {
    background: #14161a;
    border: 1px solid #2a2f38;
    border-radius: 6px;
    padding: 0.6rem;
    overflow-x: auto;
    font-size: 0.8rem;
  }
</style>
