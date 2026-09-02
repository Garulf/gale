<script>
  import { warnings, warningsDismissed } from '../warnings.js';
</script>

{#if $warnings.length > 0 && !$warningsDismissed}
  <div class="banner">
    <ul>
      {#each $warnings as warning}
        <li>{warning}</li>
      {/each}
    </ul>
    <button on:click={() => warningsDismissed.set(true)}>Dismiss</button>
  </div>
{/if}

<style>
  .banner {
    display: flex;
    align-items: flex-start;
    gap: 1rem;
    background: #3a2a12;
    border: 1px solid #a1620b;
    color: #fbbf24;
    padding: 0.6rem 0.9rem;
    margin: 0 1rem;
    border-radius: 6px;
  }

  ul {
    margin: 0;
    padding-left: 1.1rem;
    flex: 1;
  }

  button {
    background: none;
    border: 1px solid #a1620b;
    color: inherit;
    border-radius: 4px;
    padding: 0.2rem 0.6rem;
    cursor: pointer;
    flex-shrink: 0;
  }
</style>
