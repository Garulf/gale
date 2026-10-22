<script>
  import { warnings, dismissedWarnings, dismissWarning } from '../warnings.js';
  import { splitLinks } from '../linkify.js';
  import { openInGraph } from '../page.js';

  let visible = $derived($warnings.filter((warning) => !$dismissedWarnings.includes(warning)));
</script>

{#each visible as warning (warning)}
  <div class="warning-row" role="status">
    <span class="mark">!</span>
    <span class="text mono">{#each splitLinks(warning) as part}{#if part.href}<a href={part.href} target="_blank" rel="noopener">{part.href}</a>{:else}{part.text}{/if}{/each}</span>
    <button type="button" class="open" onclick={() => openInGraph({ warning })}>Open in graph</button>
    <button type="button" class="close" aria-label="Dismiss warning" onclick={() => dismissWarning(warning)}>×</button>
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
    font-size: 0.75rem;
    line-height: 1.4;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .text a {
    color: inherit;
    text-decoration: underline;
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
      padding: 0.625rem 1rem;
    }

    .open {
      display: none;
    }
  }
</style>
