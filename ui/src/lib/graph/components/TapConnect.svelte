<script>
  import { useStore } from '@xyflow/svelte';
  import { portTapState, tapPort, tapInstruction, nearestPort, portOfHandle, TAP_TARGET_PX } from '../tapConnect.js';

  let { canConnect, onConnect, resetKey } = $props();

  const HANDLE_SELECTOR = '.svelte-flow__handle';
  const CONTROLS_SELECTOR = '.svelte-flow__panel, .gale-toolbar, .gale-zoom, .gale-tap-bar';
  const store = useStore();
  let armed = $state(null);

  function handles() {
    return store.domNode ? Array.from(store.domNode.querySelectorAll(HANDLE_SELECTOR)) : [];
  }

  function markHandles(port) {
    for (const element of handles()) {
      const state = portTapState(port, portOfHandle(element), canConnect);
      if (state) element.setAttribute('data-tap', state);
      else element.removeAttribute('data-tap');
    }
  }

  function centerOf(element) {
    const rect = element.getBoundingClientRect();
    return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
  }

  function offeredNear(event) {
    const candidates = handles()
      .filter((element) => element.getAttribute('data-tap') === 'offered')
      .map((element) => ({ port: element, ...centerOf(element) }));
    return nearestPort({ x: event.clientX, y: event.clientY }, candidates);
  }

  function tappedHandle(event) {
    const direct = event.target.closest(HANDLE_SELECTOR);
    if (!armed) return direct;
    const directState = direct ? direct.getAttribute('data-tap') : '';
    if (directState === 'offered' || directState === 'armed') return direct;
    return offeredNear(event) || direct;
  }

  function swallow(event) {
    event.preventDefault();
    event.stopPropagation();
  }

  function onCanvasClick(event) {
    if (!(event.target instanceof Element) || event.target.closest(CONTROLS_SELECTOR)) return;
    const handle = tappedHandle(event);
    if (!handle) {
      if (!armed) return;
      swallow(event);
      armed = null;
      return;
    }
    swallow(event);
    const result = tapPort(armed, portOfHandle(handle), canConnect);
    armed = result.armed;
    if (result.connection) onConnect(result.connection);
  }

  function onKey(event) {
    if (!armed || event.key !== 'Escape') return;
    swallow(event);
    armed = null;
  }

  $effect(() => {
    const root = store.domNode;
    if (!root) return;
    root.addEventListener('click', onCanvasClick, true);
    return () => root.removeEventListener('click', onCanvasClick, true);
  });

  $effect(() => {
    resetKey;
    armed = null;
  });

  $effect(() => {
    const root = store.domNode;
    if (!root) return;
    root.style.setProperty('--gale-tap-hit', `${TAP_TARGET_PX / store.viewport.zoom}px`);
    root.style.setProperty('--gale-tap-line', `${2 / store.viewport.zoom}px`);
  });

  $effect(() => {
    const root = store.domNode;
    const port = armed;
    if (!root) return;
    markHandles(port);
    if (!port) return;
    let frame = 0;
    const observer = new MutationObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => markHandles(port));
    });
    observer.observe(root, { childList: true, subtree: true });
    return () => {
      observer.disconnect();
      cancelAnimationFrame(frame);
    };
  });
</script>

<svelte:window onkeydowncapture={onKey} />

<span class="gale-sr-only" role="status">{armed ? tapInstruction(armed) : ''}</span>
{#if armed}
  <div class="gale-tap-bar" data-testid="tap-connect-bar">
    <span aria-hidden="true">{tapInstruction(armed)}</span>
    <button type="button" class="btn" data-testid="tap-connect-cancel" onclick={() => (armed = null)}>Cancel</button>
  </div>
{/if}
