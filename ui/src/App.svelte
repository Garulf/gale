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
    width: 220px;
    flex-shrink: 0;
    border-right: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    padding: 20px 14px;
    gap: 6px;
    background: var(--surface);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 8px 22px;
  }

  .logo {
    width: 26px;
    height: 26px;
    filter: hue-rotate(-170deg) saturate(1.3) brightness(1.15);
  }

  .wordmark {
    font-weight: 800;
    font-size: 17px;
    letter-spacing: -0.02em;
  }

  .version {
    margin-left: auto;
    font-size: 10px;
    color: var(--faint);
  }

  .nav-gap {
    height: 14px;
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border: none;
    border-radius: var(--r);
    background: none;
    color: var(--muted);
    text-align: left;
    font-weight: 500;
    font-size: 13.5px;
  }

  .nav-item:hover {
    background: var(--surface2);
  }

  .nav-item.active {
    background: var(--surface2);
    color: var(--ink);
  }

  .nav-dot {
    width: 6px;
    height: 6px;
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
    gap: 10px;
  }

  .health {
    padding: 12px;
    border: 1px solid var(--line);
    border-radius: var(--r);
    display: grid;
    gap: 6px;
    font-size: 12px;
  }

  .health > div {
    display: flex;
    justify-content: space-between;
    gap: 8px;
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
    gap: 6px;
    color: var(--warn);
  }

  .status.live {
    color: var(--duty);
  }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--warn);
  }

  .dot.live {
    background: var(--duty);
    animation: pulse 2s infinite;
  }

  .theme {
    padding: 8px;
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
    gap: 10px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--line);
    background: var(--bg);
  }

  .topbar .logo {
    width: 22px;
    height: 22px;
  }

  .spacer {
    flex: 1;
  }

  .key-form {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 20px;
    border-bottom: 1px solid var(--line);
    font-size: 13px;
    color: var(--muted);
  }

  .key-form input {
    background: var(--surface2);
    color: var(--ink);
    border: 1px solid var(--line);
    border-radius: var(--r);
    padding: 5px 8px;
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
      left: 16px;
      right: 16px;
      bottom: calc(12px + env(safe-area-inset-bottom));
      display: grid;
      grid-template-columns: repeat(3, 1fr);
      height: 52px;
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
      gap: 3px;
      border: none;
      background: none;
      color: var(--muted);
      font-size: 11px;
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
