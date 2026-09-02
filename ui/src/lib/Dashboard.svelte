<script>
  import { snapshot } from './store.js';
</script>

<section>
  <h2>Dashboard</h2>
  {#if $snapshot === null}
    <p>Waiting for data.</p>
  {:else}
    <ul>
      {#each Object.entries($snapshot.sensors || {}) as [id, value]}
        <li>{id}: {value}</li>
      {/each}
    </ul>
  {/if}
</section>
