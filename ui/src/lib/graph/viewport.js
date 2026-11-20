export const MIN_ZOOM = 0.15;
export const MAX_FIT_ZOOM = 1.1;
export const NARROW_QUERY = '(max-width: 720px)';

const NARROW_PADDING = { top: '16px', right: '16px', bottom: '80px', left: '16px' };
const WIDE_PADDING = { top: '72px', right: '32px', bottom: '64px', left: '32px' };

export function fitOptions(narrow) {
  return { padding: narrow ? NARROW_PADDING : WIDE_PADDING, maxZoom: MAX_FIT_ZOOM, minZoom: MIN_ZOOM };
}

export function isNarrowViewport() {
  return typeof window !== 'undefined' && window.matchMedia(NARROW_QUERY).matches;
}

export function canFit(store) {
  return store.width > 0 && store.height > 0 && store.nodes.length > 0 && store.nodesInitialized;
}
