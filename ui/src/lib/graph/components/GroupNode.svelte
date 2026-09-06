<script>
  import { getContext } from 'svelte';
  import WarningBadge from './WarningBadge.svelte';
  import { Handle, Position } from '@xyflow/svelte';

  let { id, data, selected } = $props();

  const nodeWarnings = getContext('galeNodeWarnings');
  const enterGroup = getContext('galeEnterGroup');

  let group = $derived(data.group);
  let memberWarnings = $derived.by(() => {
    if (!nodeWarnings) return [];
    const all = nodeWarnings();
    return group.members.flatMap((member) => all[member] || []);
  });
  let rows = $derived(Math.max(group.inputs.length, group.outputs.length, 1));
</script>

<div
  class="node group-kind"
  class:sel={selected}
  class:warned={memberWarnings.length > 0}
  data-node-id={id}
  ondblclick={() => enterGroup && enterGroup(group.id)}
  role="button"
  tabindex="-1"
>
  <h4>
    <span class="kind"></span>
    <span class="title"><span class="name">{group.name}<WarningBadge messages={memberWarnings} /></span><small>group · {group.members.length} nodes</small></span>
  </h4>
  <div class="rows">
    {#each Array.from({ length: rows }, (_, i) => i) as index (index)}
      {@const input = group.inputs[index]}
      {@const output = group.outputs[index]}
      <div class="row both">
        {#if input}
          <Handle type="target" position={Position.Left} id={input.handle} class="port in {input.kind}" />
          <span class="in-label">{input.label}</span>
        {:else}
          <span></span>
        {/if}
        {#if output}
          <span class="out-label">{output.label}</span>
          <Handle type="source" position={Position.Right} id={output.handle} class="port out {output.kind}" />
        {/if}
      </div>
    {/each}
  </div>
  <button type="button" class="fold" data-testid="group-open" onclick={() => enterGroup && enterGroup(group.id)}>Open</button>
</div>
