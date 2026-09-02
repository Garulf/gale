<script>
  import { onDestroy } from 'svelte';
  import { connect, connected, snapshot } from './lib/store.js';
  import { page } from './lib/page.js';
  import { refreshWarnings } from './lib/warnings.js';
  import { unauthorized, saveApiKey } from './lib/auth.js';
  import WarningsBanner from './lib/components/WarningsBanner.svelte';
  import Dashboard from './pages/Dashboard.svelte';
  import Curves from './pages/Curves.svelte';
  import Config from './pages/Config.svelte';

  const disconnect = connect();
  onDestroy(disconnect);

  let keyDraft = '';

  function submitKey() {
    const key = keyDraft.trim();
    if (!key) return;
    saveApiKey(key);
    window.location.reload();
  }

  const pages = { dashboard: Dashboard, curves: Curves, config: Config };

  $: activeProfile = $snapshot ? $snapshot.active_profile : null;

  refreshWarnings();

  let wasConnected = false;
  $: {
    if ($connected && !wasConnected) {
      refreshWarnings();
    }
    wasConnected = $connected;
  }
</script>

<div class="shell">
  <nav>
    <h1>gale</h1>
    <button class:active={$page === 'dashboard'} on:click={() => page.set('dashboard')}>
      Dashboard
    </button>
    <button class:active={$page === 'curves'} on:click={() => page.set('curves')}>
      Curves
    </button>
    <button class:active={$page === 'config'} on:click={() => page.set('config')}>
      Config
    </button>
  </nav>
  <div class="main">
    <header>
      <span class="dot" class:live={$connected} class:reconnecting={!$connected}></span>
      <span class="conn-label">{$connected ? 'live' : 'reconnecting'}</span>
      {#if activeProfile}
        <span class="profile">profile: {activeProfile}</span>
      {/if}
      {#if $unauthorized}
        <form class="key-form" on:submit|preventDefault={submitKey}>
          <input type="password" placeholder="api key" bind:value={keyDraft} />
          <button type="submit">Save</button>
        </form>
      {/if}
    </header>
    <WarningsBanner />
    <main>
      <svelte:component this={pages[$page]} />
    </main>
  </div>
</div>

<style>
  :global(body) {
    margin: 0;
    background: #14161a;
    color: #e5e7eb;
    font-family: system-ui, sans-serif;
  }

  .shell {
    display: flex;
    min-height: 100vh;
  }

  nav {
    width: 180px;
    background: #1b1e24;
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  nav h1 {
    font-size: 1.1rem;
    margin: 0 0 1rem;
  }

  nav button {
    background: none;
    border: none;
    color: inherit;
    text-align: left;
    padding: 0.5rem;
    border-radius: 4px;
    cursor: pointer;
  }

  nav button.active {
    background: #2a2f38;
  }

  .main {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.75rem 1rem;
    border-bottom: 1px solid #2a2f38;
  }

  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }

  .dot.live {
    background: #22c55e;
  }

  .dot.reconnecting {
    background: #f59e0b;
  }

  .profile {
    margin-left: auto;
    opacity: 0.8;
  }

  .key-form {
    display: flex;
    gap: 0.4rem;
    margin-left: auto;
  }

  .key-form input {
    background: #14161a;
    color: inherit;
    border: 1px solid #2a2f38;
    border-radius: 4px;
    padding: 0.25rem 0.4rem;
  }

  .key-form button {
    background: #2a2f38;
    color: inherit;
    border: none;
    border-radius: 4px;
    padding: 0.25rem 0.6rem;
    cursor: pointer;
  }

  main {
    padding: 1rem;
    flex: 1;
    min-width: 0;
  }
</style>
