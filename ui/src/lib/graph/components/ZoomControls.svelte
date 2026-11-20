<script>
  import { useSvelteFlow, useViewport } from '@xyflow/svelte';
  import { fitOptions, isNarrowViewport } from '../viewport.js';

  const { zoomIn, zoomOut, fitView } = useSvelteFlow();

  const viewport = useViewport();

  let pct = $derived(`${Math.round(viewport.current.zoom * 100)}%`);
</script>

<div class="gale-zoom">
  <button type="button" aria-label="Zoom out" onclick={() => zoomOut()}>−</button>
  <button type="button" class="pct" title="Fit view" onclick={() => fitView(fitOptions(isNarrowViewport()))}>{pct}</button>
  <button type="button" aria-label="Zoom in" onclick={() => zoomIn()}>+</button>
</div>
