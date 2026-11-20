<script>
  import { warnings, dismissedWarnings, dismissWarning, describeWarning } from '../warnings.js';
  import { splitLinks } from '../linkify.js';
  import { openInGraph } from '../page.js';
  import { daemonConfig } from '../config.js';
  import { snapshot } from '../store.js';

  let visible = $derived($warnings.filter((warning) => !$dismissedWarnings.includes(warning)));

  let context = $derived.by(() => {
    const cfg = $daemonConfig;
    const profile = cfg && cfg.profiles ? cfg.profiles[cfg.active_profile] || null : null;
    return { profile, labels: cfg ? cfg.labels || null : null, duties: $snapshot ? $snapshot.duties : null };
  });

  let described = $derived(visible.map((warning) => describeWarning(warning, context)));
</script>

{#each described as entry (entry.message)}
  <div class="warning-row" role="status" data-testid="warning-row">
    <span class="mark" aria-hidden="true">!</span>
    <div class="text">
      <strong class="headline">{#each splitLinks(entry.headline) as part}{#if part.href}<a href={part.href} target="_blank" rel="noopener">{part.href}</a>{:else}{part.text}{/if}{/each}</strong>
      {#if entry.detail}
        <span class="detail">{entry.detail}</span>
      {/if}
      {#if entry.raw}
        <span class="raw mono">{entry.raw}</span>
      {/if}
    </div>
    <div class="actions">
      <button type="button" class="open" onclick={() => openInGraph({ nodeId: entry.nodeId, warning: entry.message })}>Open in graph</button>
      <button type="button" class="close" aria-label="Dismiss warning" onclick={() => dismissWarning(entry.message)}>×</button>
    </div>
  </div>
{/each}

<style>
  .warning-row {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.625rem 1.25rem;
    background: var(--warnbg);
    border-bottom: 1px solid var(--line);
    color: var(--warn);
    font-size: 0.8125rem;
  }

  .mark {
    width: 1.125rem;
    height: 1.125rem;
    border-radius: 50%;
    border: 1.5px solid var(--warn);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 0.6875rem;
    font-weight: 700;
    flex-shrink: 0;
  }

  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    column-gap: 0.625rem;
    row-gap: 0.125rem;
    line-height: 1.4;
    overflow-wrap: anywhere;
  }

  .headline {
    font-weight: 600;
  }

  .detail {
    color: var(--ink);
  }

  .raw {
    font-size: 0.75rem;
    opacity: 0.85;
  }

  .text a {
    color: inherit;
    text-decoration: underline;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    flex-shrink: 0;
  }

  .open {
    border: 1px solid var(--warn);
    background: none;
    color: var(--warn);
    border-radius: var(--r);
    padding: 0.25rem 0.625rem;
    font-size: 0.75rem;
    white-space: nowrap;
  }

  .close {
    border: none;
    background: none;
    color: var(--warn);
    font-size: 1rem;
    line-height: 1;
    padding: 2px 0.25rem;
  }

  @media (max-width: 720px) {
    .warning-row {
      display: grid;
      grid-template-columns: auto minmax(0, 1fr);
      align-items: start;
      padding: 0.625rem 1rem;
      row-gap: 0.5rem;
    }

    .mark {
      margin-top: 0.125rem;
    }

    .text {
      flex-direction: column;
      align-items: flex-start;
    }

    .actions {
      grid-column: 2;
      gap: 0.5rem;
    }

    .open {
      min-height: 2.75rem;
      padding: 0 1rem;
      font-size: 0.8125rem;
    }

    .close {
      width: 2.75rem;
      height: 2.75rem;
      font-size: 1.25rem;
      border: 1px solid transparent;
      border-radius: var(--r);
    }
  }
</style>
