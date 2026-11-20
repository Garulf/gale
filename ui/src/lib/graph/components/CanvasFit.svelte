<script>
  import { useStore, useSvelteFlow } from '@xyflow/svelte';
  import { fitOptions, canFit } from '../viewport.js';

  let { active, fitKey, narrow } = $props();

  const store = useStore();
  const { fitView } = useSvelteFlow();

  let fittedKey = null;

  $effect(() => {
    if (!active || fitKey === fittedKey || !canFit(store)) return;
    fittedKey = fitKey;
    const options = fitOptions(narrow);
    setTimeout(() => fitView(options), 0);
  });
</script>
