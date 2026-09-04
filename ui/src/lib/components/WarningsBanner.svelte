<script>
  import { warnings } from '../warnings.js';
  import { splitLinks } from '../linkify.js';
  import { openInGraph } from '../page.js';

  let dismissed = $state([]);
  let visible = $derived($warnings.filter((warning) => !dismissed.includes(warning)));

  function dismiss(warning) {
    dismissed = [...dismissed, warning];
  }
</script>

{#each visible as warning (warning)}
  <div class="warning-row" role="status">
    <span class="mark">!</span>
    <span class="text mono">{#each splitLinks(warning) as part}{#if part.href}<a href={part.href} target="_blank" rel="noopener">{part.href}</a>{:else}{part.text}{/if}{/each}</span>
    <button type="button" class="open" onclick={() => openInGraph({ warning })}>Open in graph</button>
    <button type="button" class="close" aria-label="Dismiss warning" onclick={() => dismiss(warning)}>×</button>
  </div>
{/each}

<style>
  .warning-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 20px;
    background: var(--warnbg);
    border-bottom: 1px solid var(--line);
    color: var(--warn);
    font-size: 13px;
  }

  .mark {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: 1.5px solid var(--warn);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    font-weight: 700;
    flex-shrink: 0;
  }

  .text {
    flex: 1;
    font-size: 12px;
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
    padding: 4px 10px;
    font-size: 12px;
    white-space: nowrap;
  }

  .close {
    border: none;
    background: none;
    color: var(--warn);
    font-size: 16px;
    line-height: 1;
    padding: 2px 4px;
  }

  @media (max-width: 720px) {
    .warning-row {
      padding: 10px 16px;
    }

    .open {
      display: none;
    }
  }
</style>
