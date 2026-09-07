<script>
  import { useViewport } from '@xyflow/svelte';

  let { onViewport } = $props();

  const viewport = useViewport();

  $effect(() => onViewport(viewport.current));
</script>
