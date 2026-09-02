<script>
  import { onDestroy } from 'svelte';
  import { connect, connected, snapshot } from './lib/store.js';
  import { page } from './lib/page.js';
  import Dashboard from './lib/Dashboard.svelte';
  import Curves from './lib/Curves.svelte';
  import Config from './lib/Config.svelte';

  const disconnect = connect();
  onDestroy(disconnect);

  const pages = { dashboard: Dashboard, curves: Curves, config: Config };

  $: activeProfile = $snapshot ? $snapshot.active_profile : null;
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
    </header>
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

  main {
    padding: 1rem;
  }
</style>
