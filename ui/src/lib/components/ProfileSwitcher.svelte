<script>
  import { snapshot } from '../store.js';
  import { daemonConfig, refreshConfig } from '../config.js';
  import { activateProfile } from '../api.js';
  import { refreshWarnings } from '../warnings.js';
  import { createProfile, deleteProfile } from '../api.js';
  import { page } from '../page.js';

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
    if (!window.confirm(`Create "${name}" and switch to it? It starts empty, so fans fall back to hardware control until you wire curves.`)) return;
    error = '';
    try {
      await createProfile(name);
      await activateProfile(name);
      await Promise.all([refreshConfig(), refreshWarnings()]);
      page.set('graph');
    } catch (err) {
      error = err.message;
    }
  }

  async function remove(name) {
    if (!window.confirm(`Delete profile "${name}"? This cannot be undone.`)) return;
    error = '';
    try {
      await deleteProfile(name);
      await refreshConfig();
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
        <div class="entry">
          <button type="button" role="option" aria-selected={name === active} onclick={() => pick(name)}>
            <span>{name}</span>
            {#if name === active}<span class="active mono">ACTIVE</span>{/if}
          </button>
          {#if name !== active}
            <button type="button" class="remove" aria-label="Delete profile {name}" title="Delete profile" data-testid="profile-delete-{name}" onclick={() => remove(name)}>×</button>
          {/if}
        </div>
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
    gap: 0.625rem;
    padding: 0.625rem 0.75rem;
    border: 1px solid var(--line);
    border-radius: var(--r);
    background: var(--surface2);
    color: var(--ink);
    text-align: left;
  }

  .compact .current {
    width: auto;
    height: 2.25rem;
    padding: 0 0.75rem;
    gap: 0.5rem;
    font-size: 0.75rem;
    background: var(--surface);
  }

  .dot {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    background: var(--accent);
    flex-shrink: 0;
  }

  .compact .dot {
    width: 0.375rem;
    height: 0.375rem;
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .name {
    font-weight: 600;
    font-size: 0.8125rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .compact .name {
    font-weight: 500;
    font-size: 0.75rem;
  }

  .chev {
    margin-left: auto;
    color: var(--muted);
    font-size: 0.6875rem;
  }

  .menu {
    position: absolute;
    left: 0;
    right: 0;
    top: calc(100% + 0.375rem);
    min-width: 10rem;
    background: var(--surface);
    border: 1px solid var(--line2);
    border-radius: var(--r);
    box-shadow: var(--shadow);
    padding: 0.25rem;
    z-index: 20;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .compact .menu {
    left: auto;
  }

  .entry {
    display: flex;
    align-items: stretch;
    gap: 2px;
  }

  .entry > button:first-child {
    flex: 1;
  }

  .entry .remove {
    padding: 0 0.5rem;
    color: var(--muted);
  }

  .entry .remove:hover {
    color: var(--warn);
  }

  .menu button {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 0.625rem;
    border: none;
    background: none;
    color: var(--ink);
    border-radius: var(--r);
    text-align: left;
    font-size: 0.8125rem;
  }

  .menu button span:first-child {
    flex: 1;
  }

  .menu button:hover {
    background: var(--surface2);
  }

  .active {
    font-size: 0.625rem;
    color: var(--accent);
  }

  .menu .new {
    border-top: 1px solid var(--line);
    border-radius: 0;
    margin-top: 2px;
    padding-top: 0.625rem;
    color: var(--muted);
  }

  .empty {
    padding: 0.5rem 0.625rem;
    font-size: 0.75rem;
    color: var(--muted);
  }
</style>
