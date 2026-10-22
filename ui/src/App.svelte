<script>
  import { onDestroy } from 'svelte';
  import { connect, connected } from './lib/store.js';
  import { page } from './lib/page.js';
  import { refreshWarnings } from './lib/warnings.js';
  import { daemonConfig, refreshConfig } from './lib/config.js';
  import { theme, toggleTheme } from './lib/theme.js';
  import { unauthorized, saveApiKey } from './lib/auth.js';
  import './lib/sensorHistory.js';
  import ProfileSwitcher from './lib/components/ProfileSwitcher.svelte';
  import WarningsBanner from './lib/components/WarningsBanner.svelte';
  import Dashboard from './pages/Dashboard.svelte';
  import Graph from './pages/Graph.svelte';
  import Config from './pages/Config.svelte';
  import logo from './assets/gale.svg';

  const disconnect = connect();
  onDestroy(disconnect);

  let keyDraft = $state('');

  function submitKey() {
    const key = keyDraft.trim();
    if (!key) return;
    saveApiKey(key);
    window.location.reload();
  }

  const pages = { dashboard: Dashboard, graph: Graph, config: Config };
  const navItems = [
    ['dashboard', 'Dashboard'],
    ['graph', 'Graph'],
    ['config', 'Config'],
  ];

  let Current = $derived(pages[$page]);
  let tick = $derived($daemonConfig ? `${$daemonConfig.tick_interval_ms} ms` : '—');
  let bind = $derived($daemonConfig ? $daemonConfig.api.bind : '—');

  refreshWarnings();
  refreshConfig();

  let wasConnected = false;
  $effect(() => {
    if ($connected && !wasConnected) {
      refreshWarnings();
      refreshConfig();
    }
    wasConnected = $connected;
  });
</script>

<div class="shell">
  <nav class="sidebar">
    <div class="brand">
      <img src={logo} alt="" class="logo" />
      <span class="wordmark">gale</span>
      <span class="version mono">v{__APP_VERSION__}</span>
    </div>
    <ProfileSwitcher />
    <div class="nav-gap"></div>
    {#each navItems as [key, label]}
      <button type="button" class="nav-item" class:active={$page === key} onclick={() => page.set(key)}>
        <span class="nav-dot"></span>{label}
      </button>
    {/each}
    <div class="sidebar-foot">
      <div class="health">
        <div><span>daemon</span><span class="status" class:live={$connected}><span class="dot" class:live={$connected} class:reconnecting={!$connected}></span>{$connected ? 'live' : 'reconnecting'}</span></div>
        <div><span>tick</span><span class="mono value">{tick}</span></div>
        <div><span>bind</span><span class="mono value">{bind}</span></div>
      </div>
      <button type="button" class="btn theme" onclick={toggleTheme}>{$theme === 'dark' ? '☾ Dark' : '☀ Light'}</button>
    </div>
  </nav>

  <div class="main">
    <header class="topbar">
      <img src={logo} alt="" class="logo" />
      <span class="wordmark">gale</span>
      <span class="spacer"></span>
      <ProfileSwitcher compact />
    </header>
    {#if $unauthorized}
      <form class="key-form" onsubmit={(e) => { e.preventDefault(); submitKey(); }}>
        <span>This daemon requires an API key.</span>
        <input type="password" placeholder="api key" bind:value={keyDraft} />
        <button type="submit" class="btn primary">Save</button>
      </form>
    {/if}
    <WarningsBanner />
    <Current />
  </div>

  <nav class="tabbar">
    {#each navItems as [key, label]}
      <button type="button" class:active={$page === key} onclick={() => page.set(key)}>
        <span class="nav-dot"></span>{label}
      </button>
    {/each}
  </nav>
</div>

<style>
  .shell {
    display: flex;
    height: 100vh;
    height: 100dvh;
    overflow: hidden;
  }

  .sidebar {
    width: 13.75rem;
    flex-shrink: 0;
    border-right: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    padding: 1.25rem 0.875rem;
    gap: 0.375rem;
    background: var(--surface);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 0.625rem;
    padding: 0.25rem 0.5rem 1.375rem;
  }

  .logo {
    width: 1.625rem;
    height: 1.625rem;
    filter: hue-rotate(-170deg) saturate(1.3) brightness(1.15);
  }

  .wordmark {
    font-weight: 800;
    font-size: 1.0625rem;
    letter-spacing: -0.02em;
  }

  .version {
    margin-left: auto;
    font-size: 0.625rem;
    color: var(--faint);
  }

  .nav-gap {
    height: 0.875rem;
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: 0.625rem;
    padding: 0.625rem 0.75rem;
    border: none;
    border-radius: var(--r);
    background: none;
    color: var(--muted);
    text-align: left;
    font-weight: 500;
    font-size: 0.8438rem;
  }

  .nav-item:hover {
    background: var(--surface2);
  }

  .nav-item.active {
    background: var(--surface2);
    color: var(--ink);
  }

  .nav-dot {
    width: 0.375rem;
    height: 0.375rem;
    border-radius: 50%;
    background: transparent;
  }

  .active .nav-dot {
    background: var(--accent);
  }

  .sidebar-foot {
    margin-top: auto;
    display: flex;
    flex-direction: column;
    gap: 0.625rem;
  }

  .health {
    padding: 0.75rem;
    border: 1px solid var(--line);
    border-radius: var(--r);
    display: grid;
    gap: 0.375rem;
    font-size: 0.75rem;
  }

  .health > div {
    display: flex;
    justify-content: space-between;
    gap: 0.5rem;
    color: var(--muted);
  }

  .health .value {
    color: var(--ink);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .status {
    display: inline-flex;
    align-items: center;
    gap: 0.375rem;
    color: var(--warn);
  }

  .status.live {
    color: var(--duty);
  }

  .dot {
    width: 0.375rem;
    height: 0.375rem;
    border-radius: 50%;
    background: var(--warn);
  }

  .dot.live {
    background: var(--duty);
    animation: pulse 2s infinite;
  }

  .theme {
    padding: 0.5rem;
  }

  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .topbar {
    display: none;
    align-items: center;
    gap: 0.625rem;
    padding: 0.75rem 1rem;
    border-bottom: 1px solid var(--line);
    background: var(--bg);
  }

  .topbar .logo {
    width: 1.375rem;
    height: 1.375rem;
  }

  .spacer {
    flex: 1;
  }

  .key-form {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.625rem 1.25rem;
    border-bottom: 1px solid var(--line);
    font-size: 0.8125rem;
    color: var(--muted);
  }

  .key-form input {
    background: var(--surface2);
    color: var(--ink);
    border: 1px solid var(--line);
    border-radius: var(--r);
    padding: 0.3125rem 0.5rem;
  }

  .tabbar {
    display: none;
  }

  @media (max-width: 720px) {
    .sidebar {
      display: none;
    }

    .topbar {
      display: flex;
    }

    .tabbar {
      position: fixed;
      left: 1rem;
      right: 1rem;
      bottom: calc(0.75rem + env(safe-area-inset-bottom));
      display: grid;
      grid-template-columns: repeat(3, 1fr);
      height: 3.25rem;
      border: 1px solid var(--line);
      border-radius: var(--r);
      background: var(--surface);
      z-index: 30;
    }

    .tabbar button {
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 0.1875rem;
      border: none;
      background: none;
      color: var(--muted);
      font-size: 0.6875rem;
      font-weight: 500;
      border-bottom: 2px solid transparent;
    }

    .tabbar button.active {
      color: var(--ink);
      font-weight: 600;
      border-bottom-color: var(--accent);
    }
  }
</style>
