<script>
  import { useSvelteFlow, useViewport } from '@xyflow/svelte';

  let { fitNodeIds = [] } = $props();

  const { zoomIn, zoomOut, fitView } = useSvelteFlow();

  function fitScope() {
    return fitNodeIds.length > 0 ? fitView({ nodes: fitNodeIds.map((id) => ({ id })) }) : fitView();
  }
  const viewport = useViewport();

  let pct = $derived(`${Math.round(viewport.current.zoom * 100)}%`);
</script>

<div class="gale-zoom">
  <button type="button" aria-label="Zoom out" onclick={() => zoomOut()}>−</button>
  <button type="button" class="pct" title="Fit view" onclick={() => fitScope()}>{pct}</button>
  <button type="button" aria-label="Zoom in" onclick={() => zoomIn()}>+</button>
</div>
