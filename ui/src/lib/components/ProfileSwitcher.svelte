<script>
  import { tick } from 'svelte';
  import { snapshot } from '../store.js';
  import { daemonConfig, refreshConfig } from '../config.js';
  import { activateProfile } from '../api.js';
  import { refreshWarnings } from '../warnings.js';
  import { createProfile, deleteProfile } from '../api.js';
  import { page } from '../page.js';

  let { compact = false } = $props();

  let open = $state(false);
  let managing = $state(false);
  let error = $state('');
  let root;
  let trigger;
  let active = $derived($snapshot ? $snapshot.active_profile : $daemonConfig ? $daemonConfig.active_profile : '');
  let profiles = $derived($daemonConfig ? Object.keys($daemonConfig.profiles).sort() : []);

  async function openMenu() {
    open = true;
    managing = false;
    await tick();
    const target = root.querySelector('[role="option"][aria-selected="true"]') || root.querySelector('[role="option"]');
    if (target) target.focus();
  }

  function close({ restoreFocus = false } = {}) {
    open = false;
    managing = false;
    if (restoreFocus && trigger) trigger.focus();
  }

  function toggle() {
    if (open) close();
    else openMenu();
  }

  async function pick(name) {
    close({ restoreFocus: true });
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
    close({ restoreFocus: true });
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
    if (open && !event.composedPath().includes(root)) close();
  }

  function onKeydown(event) {
    if (!open) return;
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      close({ restoreFocus: true });
      return;
    }
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      const items = Array.from(root.querySelectorAll('.menu button:not(:disabled)'));
      if (items.length === 0) return;
      event.preventDefault();
      const index = items.indexOf(document.activeElement);
      const step = event.key === 'ArrowDown' ? 1 : -1;
      items[(index + step + items.length) % items.length].focus();
    }
  }
</script>

<svelte:window onclick={onWindowClick} />

<div class="profile-switcher" class:compact bind:this={root} onkeydown={onKeydown} role="presentation">
  <button type="button" class="current" bind:this={trigger} onclick={toggle} aria-haspopup="listbox" aria-expanded={open}>
    <span class="dot"></span>
    {#if compact}
      <span class="name">{active || '…'}</span>
    {:else}
      <span class="text"><span class="eyebrow">Profile</span><span class="name">{active || '…'}</span></span>
    {/if}
    <span class="chev" aria-hidden="true">▾</span>
  </button>
  {#if open}
    <div class="menu">
      {#if managing}
        <div class="group" role="group" aria-label="Manage profiles">
          {#each profiles as name (name)}
            <div class="row manage-row">
              <span class="label">{name}</span>
              {#if name === active}
                <span class="active mono">ACTIVE</span>
              {:else}
                <button type="button" class="remove" aria-label="Delete profile {name}" data-testid="profile-delete-{name}" onclick={() => remove(name)}>Delete</button>
              {/if}
            </div>
          {/each}
        </div>
      {:else}
        <div class="group" role="listbox" aria-label="Profiles">
          {#each profiles as name (name)}
            <button type="button" class="row option" class:selected={name === active} role="option" aria-selected={name === active} onclick={() => pick(name)}>
              <span class="label">{name}</span>
              {#if name === active}<span class="active mono">ACTIVE</span>{/if}
            </button>
          {/each}
        </div>
      {/if}
      {#if profiles.length === 0}
        <span class="empty">No profiles loaded.</span>
      {/if}
      <div class="footer">
        {#if managing}
          <button type="button" class="foot" data-testid="profile-manage-done" onclick={() => (managing = false)}>Done</button>
        {:else}
          <button type="button" class="foot" data-testid="profile-new" onclick={createNew}>+ New profile…</button>
          {#if profiles.length > 1}
            <button type="button" class="foot" data-testid="profile-manage" onclick={() => (managing = true)}>Manage profiles</button>
          {/if}
        {/if}
      </div>
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
    min-width: 12rem;
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

  .group {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-height: 2.75rem;
    padding: 0 0.625rem;
    border-radius: var(--r);
    font-size: 0.8125rem;
    color: var(--ink);
  }

  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .option {
    width: 100%;
    border: none;
    background: none;
    text-align: left;
    border-left: 3px solid transparent;
  }

  .option:hover {
    background: var(--surface2);
  }

  .option.selected {
    background: var(--surface2);
    border-left-color: var(--accent);
    font-weight: 600;
  }

  .active {
    font-size: 0.625rem;
    color: var(--accent-text);
  }

  .remove {
    min-width: 4rem;
    min-height: 2rem;
    padding: 0 0.75rem;
    border: 1px solid var(--line2);
    border-radius: var(--r);
    background: none;
    color: var(--warn);
    font-size: 0.75rem;
  }

  .remove:hover {
    background: var(--surface2);
  }

  .footer {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-top: 0.25rem;
    padding-top: 0.25rem;
    border-top: 1px solid var(--line);
  }

  .foot {
    display: flex;
    align-items: center;
    min-height: 2.75rem;
    padding: 0 0.625rem;
    border: none;
    border-radius: var(--r);
    background: none;
    color: var(--muted);
    text-align: left;
    font-size: 0.8125rem;
  }

  .foot:hover {
    background: var(--surface2);
    color: var(--ink);
  }

  .empty {
    padding: 0.5rem 0.625rem;
    font-size: 0.75rem;
    color: var(--muted);
  }
</style>
