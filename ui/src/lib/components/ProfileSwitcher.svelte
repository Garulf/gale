<script>
  import { snapshot } from '../store.js';
  import { daemonConfig, refreshConfig } from '../config.js';
  import { activateProfile } from '../api.js';
  import { refreshWarnings } from '../warnings.js';
  import { createProfile } from '../api.js';
  import { openInGraph } from '../page.js';

  let { compact = false } = $props();

  let open = $state(false);
  let error = $state('');
  let active = $derived($snapshot ? $snapshot.active_profile : $daemonConfig ? $daemonConfig.active_profile : '');
  let profiles = $derived($daemonConfig ? Object.keys($daemonConfig.profiles).sort() : []);

  async function pick(name) {
    open = false;
    if (name === active) return;
    error = '';
    try {
      await activateProfile(name);
      await Promise.all([refreshConfig(), refreshWarnings()]);
    } catch (err) {
      error = err.message;
    }
  }

  async function createNew() {
    open = false;
    const name = (window.prompt('New profile name') || '').trim();
    if (!name) return;
    error = '';
    try {
      await createProfile(name);
      await refreshConfig();
      openInGraph({ profile: name });
    } catch (err) {
      error = err.message;
    }
  }

  function onWindowClick(event) {
    if (!event.target.closest('.profile-switcher')) open = false;
  }
</script>

<svelte:window onclick={onWindowClick} />

<div class="profile-switcher" class:compact>
  <button type="button" class="current" onclick={() => (open = !open)} aria-haspopup="listbox" aria-expanded={open}>
    <span class="dot"></span>
    {#if compact}
      <span class="name">{active || '…'}</span>
    {:else}
      <span class="text"><span class="eyebrow">Profile</span><span class="name">{active || '…'}</span></span>
    {/if}
    <span class="chev">▾</span>
  </button>
  {#if open}
    <div class="menu" role="listbox">
      {#each profiles as name}
        <button type="button" role="option" aria-selected={name === active} onclick={() => pick(name)}>
          <span>{name}</span>
          {#if name === active}<span class="active mono">ACTIVE</span>{/if}
        </button>
      {/each}
      {#if profiles.length === 0}
        <span class="empty">No profiles loaded.</span>
      {/if}
      <button type="button" class="new" data-testid="profile-new" onclick={createNew}>+ New profile…</button>
    </div>
  {/if}
  {#if error}
    <p class="error">{error}</p>
  {/if}
</div>

<style>
  .profile-switcher {
    position: relative;
  }

  .current {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: var(--r);
    background: var(--surface2);
    color: var(--ink);
    text-align: left;
  }

  .compact .current {
    width: auto;
    height: 36px;
    padding: 0 12px;
    gap: 8px;
    font-size: 12px;
    background: var(--surface);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
    flex-shrink: 0;
  }

  .compact .dot {
    width: 6px;
    height: 6px;
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .name {
    font-weight: 600;
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .compact .name {
    font-weight: 500;
    font-size: 12px;
  }

  .chev {
    margin-left: auto;
    color: var(--muted);
    font-size: 11px;
  }

  .menu {
    position: absolute;
    left: 0;
    right: 0;
    top: calc(100% + 6px);
    min-width: 160px;
    background: var(--surface);
    border: 1px solid var(--line2);
    border-radius: var(--r);
    box-shadow: var(--shadow);
    padding: 4px;
    z-index: 20;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .compact .menu {
    left: auto;
  }

  .menu button {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border: none;
    background: none;
    color: var(--ink);
    border-radius: var(--r);
    text-align: left;
    font-size: 13px;
  }

  .menu button span:first-child {
    flex: 1;
  }

  .menu button:hover {
    background: var(--surface2);
  }

  .active {
    font-size: 10px;
    color: var(--accent);
  }

  .menu .new {
    border-top: 1px solid var(--line);
    border-radius: 0;
    margin-top: 2px;
    padding-top: 10px;
    color: var(--muted);
  }

  .empty {
    padding: 8px 10px;
    font-size: 12px;
    color: var(--muted);
  }
</style>
