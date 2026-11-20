<script>
  import { useStore, useSvelteFlow } from '@xyflow/svelte';
  import { canFit, canvasInset, revealViewport } from '../viewport.js';
  import { allFinished, createRevealSchedule, layoutTransitions } from '../revealSchedule.js';

  let { request, nodeIds, active } = $props();

  const store = useStore();
  const { getNodesBounds, getInternalNode, getViewport, setViewport } = useSvelteFlow();

  let handled = 0;

  function canvasSize() {
    const element = store.domNode;
    if (element && element.clientWidth > 0) return { width: element.clientWidth, height: element.clientHeight };
    return { width: store.width, height: store.height };
  }

  function reveal() {
    const present = nodeIds.filter((id) => getInternalNode(id));
    if (present.length === 0) return;
    const next = revealViewport(getNodesBounds(present), getViewport(), canvasSize(), canvasInset(false));
    if (next) setViewport(next);
  }

  const schedule = createRevealSchedule(reveal);

  $effect(() => {
    if (!request || request === handled || !active || !canFit(store)) return;
    handled = request;
    schedule.request(allFinished(layoutTransitions(store.domNode)));
  });

  $effect(() => {
    store.width;
    store.height;
    schedule.resized();
  });

  $effect(() => () => schedule.cancel());
</script>
